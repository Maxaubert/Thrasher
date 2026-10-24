//! A small timing log, `thrasher.log` in the app's local data folder, so slow starts,
//! slow clips and freezes can be found afterwards. Each line also goes to stderr.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// The log is started again when it grows past this.
const MAX_BYTES: u64 = 1024 * 1024;

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "Thrasher")
        .map(|d| d.data_local_dir().join("thrasher.log"))
}

/// Opens the log. Call first thing in `main`: times are counted from here.
pub fn init() {
    START.get_or_init(Instant::now);
    let Some(p) = path() else { return };
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
    let fresh = std::fs::metadata(&p).is_ok_and(|m| m.len() > MAX_BYTES);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(!fresh)
        .write(true)
        .truncate(fresh)
        .open(&p)
        .ok();
    *FILE.lock().unwrap() = file;
}

/// Milliseconds since the app started.
pub fn uptime_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

pub fn write(line: &str) {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("{stamp} [{:>6} ms] {line}", uptime_ms());
    eprintln!("{line}");
    if let Some(f) = FILE.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{line}");
    }
}

/// `log!("loaded {name} in {} ms", t.elapsed().as_millis())`
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}

/// Milliseconds since `t`, for log lines.
pub fn ms(t: Instant) -> u128 {
    t.elapsed().as_millis()
}
