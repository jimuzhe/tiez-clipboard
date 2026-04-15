use chrono::{DateTime, Utc};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init(path: PathBuf) {
    let mut guard = LOG_PATH.lock().unwrap();
    *guard = Some(path);
}

pub fn log(msg: &str) {
    if let Ok(guard) = LOG_PATH.lock() {
        if let Some(path) = &*guard {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                let now: DateTime<Utc> = SystemTime::now().into();
                let timestamp = now.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                let _ = writeln!(f, "[{}] {}", timestamp, msg);
            }
        }
    }
    // Also print to console for development
    println!("{}", msg);
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logger::log(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logger::log(&format!("[ERROR] {}", format!($($arg)*)))
    };
}
