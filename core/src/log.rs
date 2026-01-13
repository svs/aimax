//! Structured Logging - S-expression format for Scheme consumption
//!
//! Format: (log LEVEL MODULE EVENT "TIMESTAMP" ((key . "value") ...))
//!
//! Levels: debug, info, warn, error
//! Modules: editor, buffer, keymap, scheme, llm, process, ipc, etc.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
static ECHO_STDOUT: AtomicBool = AtomicBool::new(false);

/// Enable echoing logs to stdout (for headless mode)
pub fn set_echo_stdout(enabled: bool) {
    ECHO_STDOUT.store(enabled, Ordering::Relaxed);
}

/// Initialize the log file (call once at startup)
pub fn init() {
    if let Ok(mut guard) = LOG_FILE.lock() {
        if guard.is_none() {
            if let Ok(f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/aimax.log")
            {
                *guard = Some(f);
            }
        }
    }
}

/// Log a structured message
///
/// # Arguments
/// * `level` - debug, info, warn, error
/// * `module` - editor, buffer, keymap, scheme, llm, process, ipc, etc.
/// * `event` - what happened (kebab-case: "file-opened", "key-pressed", etc.)
/// * `data` - key-value pairs for context
pub fn log(level: &str, module: &str, event: &str, data: &[(&str, &str)]) {
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    let fields: Vec<String> = data.iter()
        .map(|(k, v)| format!("({} . \"{}\")", k, v.replace("\\", "\\\\").replace("\"", "\\\"")))
        .collect();

    let line = format!("(log {} {} {} \"{}\" ({}))\n",
        level, module, event, timestamp, fields.join(" "));

    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut f) = *guard {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }

    // Echo to stdout if enabled (headless mode)
    if ECHO_STDOUT.load(Ordering::Relaxed) {
        print!("{}", line);
        let _ = std::io::stdout().flush();
    }
}

/// Convenience macros for different log levels
#[macro_export]
macro_rules! log_debug {
    ($module:expr, $event:expr, $($key:expr => $val:expr),* $(,)?) => {
        $crate::log::log("debug", $module, $event, &[$(($key, $val)),*])
    };
}

#[macro_export]
macro_rules! log_info {
    ($module:expr, $event:expr, $($key:expr => $val:expr),* $(,)?) => {
        $crate::log::log("info", $module, $event, &[$(($key, $val)),*])
    };
}

#[macro_export]
macro_rules! log_warn {
    ($module:expr, $event:expr, $($key:expr => $val:expr),* $(,)?) => {
        $crate::log::log("warn", $module, $event, &[$(($key, $val)),*])
    };
}

#[macro_export]
macro_rules! log_error {
    ($module:expr, $event:expr, $($key:expr => $val:expr),* $(,)?) => {
        $crate::log::log("error", $module, $event, &[$(($key, $val)),*])
    };
}
