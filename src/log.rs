use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static LOG_FILE: Mutex<Option<String>> = Mutex::new(None);
static VERBOSE: AtomicBool = AtomicBool::new(false);
static START: OnceLock<Instant> = OnceLock::new();

pub fn init() {
    START.get_or_init(Instant::now);
    let dir = format!("{}/.cliphop", std::env::var("HOME").unwrap_or_default());
    let _ = fs::create_dir_all(&dir);
    let path = format!("{}/log", dir);
    // Truncate on startup
    let _ = fs::write(&path, "");
    *LOG_FILE.lock().unwrap() = Some(path);
}

/// Always writes to the log file (for startup messages, errors, and important events).
pub fn log(msg: &str) {
    write_log(msg);
}

/// Only writes when verbose logging is enabled (for detailed debug info).
pub fn log_verbose(msg: &str) {
    if !VERBOSE.load(Ordering::Relaxed) {
        return;
    }
    write_log(msg);
}

fn write_log(msg: &str) {
    let guard = LOG_FILE.lock().unwrap();
    let Some(path) = guard.as_ref() else { return };
    let elapsed = START.get().map(|s| s.elapsed()).unwrap_or_default();
    let secs = elapsed.as_secs();
    let millis = elapsed.subsec_millis();
    let line = format!("[+{secs}.{millis:03}] {msg}\n");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub fn set_verbose(enabled: bool) {
    VERBOSE.store(enabled, Ordering::Relaxed);
}

pub fn log_path() -> String {
    let guard = LOG_FILE.lock().unwrap();
    guard.as_deref().unwrap_or_default().to_owned()
}
