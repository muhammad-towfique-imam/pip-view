//! A small log file for problem reports: `log.txt` next to `config.json`.
//! The app starts a new file at each start.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static FILE: OnceLock<Mutex<File>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// Milliseconds since the app started.
pub fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Opens the log file. Without a call, `write` does nothing.
pub fn init(path: &Path) {
    START.get_or_init(Instant::now);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(f) = File::create(path) {
        let _ = FILE.set(Mutex::new(f));
    }
}

/// 1 log line: the time in seconds, the thread name, and the message.
pub fn format_line(ms: u64, thread: &str, msg: &str) -> String {
    format!("{:>4}.{:03} [{}] {}\n", ms / 1000, ms % 1000, thread, msg)
}

pub fn write(args: std::fmt::Arguments) {
    let Some(file) = FILE.get() else { return };
    let thread = std::thread::current();
    let line = format_line(now_ms(), thread.name().unwrap_or("?"), &args.to_string());
    if let Ok(mut f) = file.lock() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

/// Writes a line to the log file, with `format!` arguments.
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}
pub(crate) use log;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_time_thread_and_message() {
        assert_eq!(format_line(12_345, "main", "camera open"), "  12.345 [main] camera open\n");
        assert_eq!(format_line(7, "capture", "x"), "   0.007 [capture] x\n");
    }
}
