//! Aimax TUI - Terminal interface
//!
//! Thin rendering layer over the testable core Editor.

use std::env;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::fs::OpenOptions;

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
    widgets::{List, ListItem, Paragraph, Wrap},
    Terminal,
};

use aimax_core::{
    Buffer, Editor, Key, KeyLookup, KeymapStack,
    Lang, SyntaxHighlighter,
};

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

/// Spinner frames for AI thinking indicator
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// TUI state wrapping the core Editor
struct Tui {
    editor: Editor,
    keymaps: KeymapStack,
    syntax: SyntaxHighlighter,
    should_quit: bool,
    needs_redraw: bool,
    /// Frame counter for spinner animation
    frame: usize,
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
            frame: 0,
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

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, tui: &mut Tui) -> io::Result<()> {
    loop {
        // Handle IPC commands
        if tui.editor.process_ipc() {
            tui.needs_redraw = true;
        }

        // Handle process output (PTY reader threads)
        if tui.editor.process_messages() {
            tui.needs_redraw = true;
        }

        // Handle chat stream (Claude responses)
        if tui.editor.process_chat() {
            tui.needs_redraw = true;
        }

        // Animate spinner while streaming
        if tui.editor.chat_streaming {
            tui.frame = tui.frame.wrapping_add(1);
            tui.needs_redraw = true;
        }

        // Apply any pending face changes
        for (face, key, value) in tui.editor.face_actions.drain(..) {
            if let Err(e) = tui.syntax.faces.set_attribute(&face, &key, &value) {
                log(&format!("Face error: {}", e));
            }
            tui.needs_redraw = true;
        }

        // Apply any pending keybindings from Scheme
        for (key, command) in tui.editor.pending_keybindings.drain(..) {
            tui.keymaps.global_mut().bind(&key, &command);
        }

        // Render
        if tui.needs_redraw {
            tui.needs_redraw = false;

            let lang = tui.buffer_lang();
            let text = tui.editor.buffer_ref().text();
            let buffer_name = tui.editor.buffer_ref().name.clone();
            let cursor_line = tui.editor.buffer_ref().current_line();
            let cursor_col = tui.editor.buffer_ref().current_column();
            let modified = tui.editor.buffer_ref().is_modified();
            let status_msg = tui.editor.status_message.clone();
            let chat_streaming = tui.editor.chat_streaming;

            // Minibuffer state
            let mb_active = tui.editor.minibuffer_active;
            let mb_prompt = tui.editor.minibuffer_prompt.clone();
            let mb_input = tui.editor.minibuffer_input.clone();
            let mb_matches = tui.editor.minibuffer_matches.clone();
            let mb_selected = tui.editor.minibuffer_selected;

            // Get minibuffer-current face for selected item styling
            let mb_current_face = tui.syntax.faces.get_or_default("minibuffer-current");

            terminal.draw(|f| {
                let size = f.size();

                // Layout: buffer + status + minibuffer
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(1),
                        Constraint::Length(1),
                        Constraint::Length(if mb_active { (mb_matches.len().min(10) + 1) as u16 } else { 1 }),
                    ])
                    .split(size);

                // Calculate scroll offset to keep cursor visible
                let height = chunks[0].height as usize;
                let scroll_offset = if cursor_line > height {
                    cursor_line - height
                } else {
                    0
                };

                // Highlight visible lines
                let start_line = scroll_offset + 1;
                let end_line = scroll_offset + height;
                let line_highlights = tui.syntax.highlights_for_lines(lang, &text, start_line, end_line);

                // Render buffer with syntax highlighting
                render_buffer(f, chunks[0], &text, &line_highlights, cursor_line, cursor_col, scroll_offset);

                // Status line
                let mod_indicator = if modified { "[+] " } else { "" };
                let ai_indicator = if chat_streaming {
                    let spinner = SPINNER[tui.frame % SPINNER.len()];
                    format!(" {} AI thinking...", spinner)
                } else {
                    String::new()
                };
                let status = format!(" {}{} L{}:C{}{}{}",
                    mod_indicator,
                    buffer_name,
                    cursor_line,
                    cursor_col,
                    ai_indicator,
                    status_msg.as_deref().map(|s| format!(" {}", s)).unwrap_or_default()
                );
                let status_style = if chat_streaming {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                };
                let status_widget = Paragraph::new(status).style(status_style);
                f.render_widget(status_widget, chunks[1]);

                // Minibuffer
                if mb_active {
                    render_minibuffer(f, chunks[2], &mb_prompt, &mb_input, &mb_matches, mb_selected, &mb_current_face);
                } else {
                    let mini = Paragraph::new(status_msg.as_deref().unwrap_or(""));
                    f.render_widget(mini, chunks[2]);
                }
            })?;
        }

        // Handle input
        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key_event) => {
                    tui.needs_redraw = true;
                    if tui.editor.minibuffer_active {
                        handle_minibuffer_key(tui, &key_event);
                    } else {
                        handle_buffer_key(tui, &key_event);
                    }
                }
                Event::Resize(_, _) => {
                    tui.needs_redraw = true;
                }
                _ => {}
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
    scroll_offset: usize,
) {
    let height = area.height as usize;
    let mut lines: Vec<Line> = Vec::new();

    for (idx, line_text) in text.lines().skip(scroll_offset).take(height).enumerate() {
        let line_highlights = highlights.get(idx).cloned().unwrap_or_default();

        let mut spans: Vec<Span> = Vec::new();
        let mut last_end = 0;

        for (start, end, color) in line_highlights {
            // Add unstyled text before this highlight
            if start > last_end && last_end < line_text.len() {
                let end_idx = start.min(line_text.len());
                if let Some(slice) = line_text.get(last_end..end_idx) {
                    spans.push(Span::raw(slice.to_string()));
                }
            }
            // Add highlighted text
            if start < line_text.len() {
                let end_idx = end.min(line_text.len());
                if let Some(slice) = line_text.get(start..end_idx) {
                    let (r, g, b) = color;
                    spans.push(Span::styled(
                        slice.to_string(),
                        Style::default().fg(Color::Rgb(r, g, b))
                    ));
                }
            }
            last_end = end;
        }

        // Add remaining unstyled text
        if last_end < line_text.len() {
            if let Some(slice) = line_text.get(last_end..) {
                spans.push(Span::raw(slice.to_string()));
            }
        }

        if spans.is_empty() {
            spans.push(Span::raw(line_text.to_string()));
        }

        lines.push(Line::from(spans));
    }

    let buffer_widget = Paragraph::new(lines)
        .wrap(Wrap { trim: false });
    f.render_widget(buffer_widget, area);

    // Position cursor (adjusted for scroll)
    let cursor_screen_line = cursor_line.saturating_sub(scroll_offset);
    if cursor_screen_line > 0 && cursor_screen_line <= height {
        f.set_cursor(
            area.x + cursor_col as u16,
            area.y + (cursor_screen_line - 1) as u16
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
    current_face: &aimax_core::FaceAttributes,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let prompt_line = format!("{}{}", prompt, input);
    let prompt_widget = Paragraph::new(prompt_line);
    f.render_widget(prompt_widget, chunks[0]);

    if !matches.is_empty() && chunks[1].height > 0 {
        // Convert face to ratatui Style
        let selected_style = {
            let mut style = Style::default();
            if let Some(bg) = current_face.bg {
                style = style.bg(Color::Rgb(bg.r, bg.g, bg.b));
            }
            if let Some(fg) = current_face.fg {
                style = style.fg(Color::Rgb(fg.r, fg.g, fg.b));
            }
            style
        };

        let items: Vec<ListItem> = matches.iter()
            .enumerate()
            .take(chunks[1].height as usize)
            .map(|(i, m)| {
                let style = if i == selected {
                    selected_style
                } else {
                    Style::default()
                };
                ListItem::new(m.as_str()).style(style)
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, chunks[1]);
    }

    f.set_cursor(
        chunks[0].x + prompt.len() as u16 + input.len() as u16,
        chunks[0].y
    );
}

fn handle_minibuffer_key(tui: &mut Tui, key: &KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let _ = tui.editor.minibuffer_submit();
        }
        KeyCode::Esc | KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.minibuffer_cancel();
        }
        KeyCode::Tab => {
            tui.editor.minibuffer_complete();
        }
        KeyCode::Down => {
            tui.editor.minibuffer_next();
        }
        KeyCode::Up => {
            tui.editor.minibuffer_prev();
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.editor.minibuffer_next();
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
        _ => {}
    }
}

fn handle_buffer_key(tui: &mut Tui, key: &KeyEvent) {
    // Check keymap first for commands like C-x C-f
    if let Some(our_key) = key_from_event(key) {
        match tui.keymaps.process_key(&our_key) {
            KeyLookup::Command(cmd) => {
                execute_command(tui, &cmd);
                return;
            }
            KeyLookup::Prefix => {
                tui.editor.status_message = Some(format!("{}-", tui.keymaps.pending_display()));
                return;
            }
            KeyLookup::Unbound => {
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
        // Page Up/Down
        KeyCode::PageUp => {
            let height = 20; // Approximate, could get from terminal size
            for _ in 0..height {
                tui.editor.buffer().previous_line();
            }
        }
        KeyCode::PageDown => {
            let height = 20;
            for _ in 0..height {
                tui.editor.buffer().next_line();
            }
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

        // Keyboard quit
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            tui.keymaps.clear_pending();
            tui.editor.status_message = Some("Quit".to_string());
        }

        _ => {}
    }
}

fn key_from_event(event: &KeyEvent) -> Option<Key> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    // Use Alt or Cmd (Super) for Meta
    let alt = event.modifiers.contains(KeyModifiers::ALT)
           || event.modifiers.contains(KeyModifiers::SUPER);

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
        KeyCode::PageUp => "prior".to_string(),
        KeyCode::PageDown => "next".to_string(),
        KeyCode::Esc => "escape".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => return None,
    };

    Some(Key::new(ctrl, alt, &code))
}

fn execute_command(tui: &mut Tui, cmd: &str) {
    use aimax_core::CommandResult;

    if cmd == "keyboard-quit" {
        tui.keymaps.clear_pending();
    }

    match tui.editor.execute_command(cmd) {
        CommandResult::Quit => tui.should_quit = true,
        _ => {}
    }
}
