use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

static LOG_FILE: Mutex<Option<String>> = Mutex::new(None);
static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let dir = format!("{}/.cliphop", std::env::var("HOME").unwrap_or_default());
    let _ = fs::create_dir_all(&dir);
    let path = format!("{}/log", dir);
    // Truncate on startup
    let _ = fs::write(&path, "");
    *LOG_FILE.lock().unwrap() = Some(path);
}

pub fn log(msg: &str) {
    if !VERBOSE.load(Ordering::Relaxed) {
        return;
    }
    let guard = LOG_FILE.lock().unwrap();
    let Some(path) = guard.as_ref() else { return };
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let line = format!("[{secs}.{millis:03}] {msg}\n");
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
    guard.clone().unwrap_or_default()
}
