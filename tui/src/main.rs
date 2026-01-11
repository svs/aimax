//! Aimax TUI - Terminal interface
//!
//! Thin rendering layer over the testable core Editor.

use std::env;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::fs::{self, OpenOptions};
use std::time::SystemTime;

/// Log to /tmp/aimax.log
fn log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/aimax.log")
    {
        let _ = writeln!(file, "{}", msg);
    }
}
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
    Terminal,
};

use aimax_core::{
    Buffer, Editor, Key, KeyLookup, KeymapStack,
    Lang, MinibufferMode, SyntaxHighlighter,
};

/// TUI state wrapping the core Editor
struct Tui {
    editor: Editor,
    keymaps: KeymapStack,
    syntax: SyntaxHighlighter,
    should_quit: bool,
    needs_redraw: bool,
}

impl Tui {
    fn new(file_path: Option<&str>) -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut editor = Editor::new(cwd);

        // Start IPC server
        aimax_core::ipc::start_server(editor.ipc_tx.clone());

        // Load file if provided
        if let Some(path) = file_path {
            match Buffer::from_file(path) {
                Ok(buf) => editor.buffers[0] = buf,
                Err(e) => eprintln!("Warning: Could not load {}: {}", path, e),
            }
        }

        Tui {
            editor,
            keymaps: KeymapStack::new(),
            syntax: SyntaxHighlighter::new(),
            should_quit: false,
            needs_redraw: true,
        }
    }

    fn buffer_lang(&self) -> Lang {
        self.editor.buffer_ref().file_path.as_ref()
            .map(|p| Lang::from_path(p))
            .unwrap_or(Lang::Plain)
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(|s| s.as_str());

    let mut tui = Tui::new(file_path);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut tui);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }
    Ok(())
}

const EVAL_FILE: &str = "/tmp/aimax-eval.scm";
const RESULT_FILE: &str = "/tmp/aimax-result.txt";

fn check_eval_file(tui: &mut Tui, last_mtime: &mut Option<SystemTime>) {
    let mtime = fs::metadata(EVAL_FILE).ok().and_then(|m| m.modified().ok());

    if mtime != *last_mtime && mtime.is_some() {
        *last_mtime = mtime;

        if let Ok(code) = fs::read_to_string(EVAL_FILE) {
            let code = code.trim();
            if !code.is_empty() {
                log(&format!("EVAL: {}", code));
                let result = match tui.editor.scheme.run(code) {
                    Ok(val) => format!("{:?}", val),
                    Err(e) => format!("ERROR: {}", e),
                };
                log(&format!("RESULT: {}", result));
                let _ = fs::write(RESULT_FILE, &result);
                tui.editor.status_message = Some(format!("Eval: {}", &result[..result.len().min(50)]));
            }
        }
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, tui: &mut Tui) -> io::Result<()> {
    loop {
        // Handle IPC commands (may set needs_redraw)
        if tui.editor.process_ipc() {
            tui.needs_redraw = true;
        }

        // Apply any pending face changes
        for (face, key, value) in tui.editor.face_actions.drain(..) {
            if let Err(e) = tui.syntax.faces.set_attribute(&face, &key, &value) {
                log(&format!("Face error: {}", e));
            }
            tui.needs_redraw = true;
        }

        // Only render when needed
        if tui.needs_redraw {
            tui.needs_redraw = false;

            // Clone state for rendering
            let lang = tui.buffer_lang();
            let text = tui.editor.buffer_ref().text();
            let buffer_name = tui.editor.buffer_ref().name.clone();
            let cursor_line = tui.editor.buffer_ref().current_line();
            let cursor_col = tui.editor.buffer_ref().current_column();
            let status_msg = tui.editor.status_message.clone();

            // Minibuffer state from Scheme
            let mb_active = tui.editor.minibuffer_active;
            let mb_prompt = tui.editor.minibuffer_prompt.clone();
            let mb_input = tui.editor.minibuffer_input.clone();
            let mb_matches = tui.editor.minibuffer_matches.clone();
            let mb_selected = tui.editor.minibuffer_selected;

            terminal.draw(|f| {
                let size = f.size();

                // Layout: buffer area + status line + minibuffer
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(1),
                        Constraint::Length(1),
                        Constraint::Length(if mb_active { (mb_matches.len().min(10) + 1) as u16 } else { 1 }),
                    ])
                    .split(size);

                // Highlight visible lines in ONE pass (not per-line!)
                let visible_lines = chunks[0].height as usize;
                let line_highlights = tui.syntax.highlights_for_lines(lang, &text, 1, visible_lines);

                // Render buffer with syntax highlighting
                render_buffer(f, chunks[0], &text, &line_highlights, cursor_line, cursor_col);

                // Status line
                let status = format!(" {} L{}:C{} {}",
                    buffer_name,
                    cursor_line,
                    cursor_col,
                    status_msg.as_deref().unwrap_or("")
                );
                let status_widget = Paragraph::new(status)
                    .style(Style::default().bg(Color::DarkGray).fg(Color::White));
                f.render_widget(status_widget, chunks[1]);

                // Minibuffer
                if mb_active {
                    render_minibuffer(f, chunks[2], &mb_prompt, &mb_input, &mb_matches, mb_selected);
                } else {
                    let mini = Paragraph::new(status_msg.as_deref().unwrap_or(""));
                    f.render_widget(mini, chunks[2]);
                }
            })?;
        }

        // Handle input - poll with short timeout to stay responsive to IPC
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key_event) = event::read()? {
                tui.needs_redraw = true;
                if tui.editor.minibuffer_active {
                    handle_minibuffer_key(tui, &key_event);
                } else {
                    handle_buffer_key(tui, &key_event);
                }
            }
        }

        if tui.should_quit {
            break;
        }
    }
    Ok(())
}

fn render_buffer(
    f: &mut ratatui::Frame,
    area: Rect,
    text: &str,
    highlights: &[Vec<(usize, usize, (u8, u8, u8))>],
    cursor_line: usize,
    cursor_col: usize,
) {
    let mut lines: Vec<Line> = Vec::new();

    for (line_idx, line_text) in text.lines().enumerate().take(area.height as usize) {
        let line_highlights = highlights.get(line_idx).cloned().unwrap_or_default();

        let mut spans: Vec<Span> = Vec::new();
        let mut last_end = 0;

        for (start, end, color) in line_highlights {
            if start > last_end {
                spans.push(Span::raw(&line_text[last_end..start.min(line_text.len())]));
            }
            if start < line_text.len() {
                let (r, g, b) = color;
                spans.push(Span::styled(
                    &line_text[start..end.min(line_text.len())],
                    Style::default().fg(Color::Rgb(r, g, b))
                ));
            }
            last_end = end;
        }

        if last_end < line_text.len() {
            spans.push(Span::raw(&line_text[last_end..]));
        }

        if spans.is_empty() {
            spans.push(Span::raw(line_text));
        }

        lines.push(Line::from(spans));
    }

    let buffer_widget = Paragraph::new(lines);
    f.render_widget(buffer_widget, area);

    // Position cursor
    if cursor_line <= area.height as usize {
        f.set_cursor(
            area.x + cursor_col.saturating_sub(1) as u16,
            area.y + cursor_line.saturating_sub(1) as u16
        );
    }
}

fn render_minibuffer(
    f: &mut ratatui::Frame,
    area: Rect,
    prompt: &str,
    input: &str,
    matches: &[String],
    selected: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    // Prompt + input
    let prompt_line = format!("{}{}", prompt, input);
    let prompt_widget = Paragraph::new(prompt_line);
    f.render_widget(prompt_widget, chunks[0]);

    // Completions
    if !matches.is_empty() && chunks[1].height > 0 {
        let items: Vec<ListItem> = matches.iter()
            .enumerate()
            .take(chunks[1].height as usize)
            .map(|(i, m)| {
                let style = if i == selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };
                ListItem::new(m.as_str()).style(style)
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, chunks[1]);
    }

    // Cursor in minibuffer
    f.set_cursor(
        chunks[0].x + prompt.len() as u16 + input.len() as u16,
        chunks[0].y
    );
}

fn handle_minibuffer_key(tui: &mut Tui, key: &KeyEvent) {
    log(&format!("minibuffer key: {:?}, active={}, input={:?}",
        key.code, tui.editor.minibuffer_active, tui.editor.minibuffer_input));

    match key.code {
        KeyCode::Enter => {
            log(&format!("ENTER pressed, input={:?}", tui.editor.minibuffer_input));
            match tui.editor.minibuffer_submit() {
                Ok(()) => {
                    let name = tui.editor.buffer_ref().name.clone();
                    log(&format!("submit OK, buffer={}, status={:?}", name, tui.editor.status_message));
                }
                Err(e) => log(&format!("submit ERROR: {}", e)),
            }
        }
        KeyCode::Esc => {
            tui.editor.minibuffer_cancel();
        }
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.minibuffer_cancel();
        }
        KeyCode::Tab => {
            tui.editor.minibuffer_complete();
            log(&format!("TAB complete, input={:?}", tui.editor.minibuffer_input));
        }
        // Arrow keys always work, C-n/C-p also work
        KeyCode::Down => {
            tui.editor.minibuffer_next();
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.minibuffer_next();
        }
        KeyCode::Up => {
            tui.editor.minibuffer_prev();
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.minibuffer_prev();
        }
        KeyCode::Backspace => {
            tui.editor.minibuffer_delete_backward();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.minibuffer_insert(c);
        }
        _ => {
            log(&format!("unhandled minibuffer key: {:?}", key.code));
        }
    }
}

fn handle_buffer_key(tui: &mut Tui, key: &KeyEvent) {
    // Check keymap first - use process_key for key sequences like C-x C-f
    if let Some(our_key) = key_from_event(key) {
        match tui.keymaps.process_key(&our_key) {
            KeyLookup::Command(cmd) => {
                execute_command(tui, &cmd);
                return;
            }
            KeyLookup::Prefix => {
                // Show pending keys in status
                tui.editor.status_message = Some(format!("{}-", tui.keymaps.pending_display()));
                return;
            }
            KeyLookup::Unbound => {
                // If we had pending keys, clear them and show error
                if tui.keymaps.has_pending() {
                    let pending = tui.keymaps.pending_display();
                    tui.keymaps.clear_pending();
                    tui.editor.status_message = Some(format!("{} {} is undefined", pending, our_key.as_str()));
                    return;
                }
            }
        }
    }

    // Default key handling
    match key.code {
        // Movement
        KeyCode::Left | KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().backward_char();
        }
        KeyCode::Right | KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().forward_char();
        }
        KeyCode::Up | KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().previous_line();
        }
        KeyCode::Down | KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().next_line();
        }
        KeyCode::Home | KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().beginning_of_line();
        }
        KeyCode::End | KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().end_of_line();
        }

        // Editing
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) => {
            tui.editor.buffer().insert_char(c);
        }
        KeyCode::Enter => {
            tui.editor.buffer().insert_char('\n');
        }
        KeyCode::Backspace => {
            tui.editor.buffer().delete_backward();
        }
        KeyCode::Delete | KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.buffer().delete_forward();
        }

        // Commands
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.keymaps.clear_pending();
            tui.editor.status_message = Some("Quit".to_string());
        }

        _ => {}
    }
}

fn key_from_event(event: &KeyEvent) -> Option<Key> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let alt = event.modifiers.contains(KeyModifiers::ALT);

    let code = match event.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "return".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::Esc => "escape".to_string(),
        _ => return None,
    };

    Some(Key::new(ctrl, alt, &code))
}

fn execute_command(tui: &mut Tui, cmd: &str) {
    use aimax_core::CommandResult;
    log(&format!("execute_command: {}", cmd));

    // TUI-specific handling for keyboard-quit (clears pending keys)
    if cmd == "keyboard-quit" {
        tui.keymaps.clear_pending();
    }

    // Core handles everything else
    match tui.editor.execute_command(cmd) {
        CommandResult::Quit => tui.should_quit = true,
        _ => {}
    }
}
