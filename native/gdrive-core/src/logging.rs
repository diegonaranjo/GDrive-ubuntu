//! Logging with rotation + activity log. Reimplements `log_write` and
//! `write_activity` from `gdrive/helpers.py` as a single canonical
//! implementation (the Python had it triplicated and divergent).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{config_home, Config};

/// Severity level of a log message, ordered from least to most detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Error (the highest priority).
    Error = 0,
    /// Information about normal operation.
    Info = 1,
    /// Debugging detail.
    Debug = 2,
    /// Exhaustive trace.
    Trace = 3,
}

impl FromStr for Level {
    type Err = std::convert::Infallible;

    /// Converts a string (`"error"`, `"debug"`, `"trace"`) into a [`Level`].
    /// Any unrecognized value is interpreted as [`Level::Info`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "error" => Level::Error,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => Level::Info,
        })
    }
}

impl Level {
    /// Parses the level from a string with an `Info` fallback (convenience over
    /// [`FromStr::from_str`]).
    pub fn parse(s: &str) -> Level {
        s.parse().unwrap_or(Level::Info)
    }
}

/// Writes a line to `debug.log` and, optionally, to `activity.log`.
/// Filters by level according to `config.log_level`.
pub fn log_write(cfg: &Config, level: Level, component: &str, message: &str, also_activity: bool) {
    if !cfg.logging_enabled {
        return;
    }
    let min_level = Level::parse(&cfg.log_level);
    if level > min_level && !also_activity {
        return;
    }

    let line = format!(
        "[{}] [{:5}] [{}] {}\n",
        timestamp_log(),
        level_name(level).to_uppercase(),
        component,
        message
    );

    let dir = log_dir();
    fs::create_dir_all(&dir).ok();

    if cfg.log_rotate {
        maybe_rotate(cfg, &dir);
    }

    append(&dir.join("debug.log"), &line);

    if also_activity {
        write_activity(component, message);
    }
}

/// Appends an event to `activity.log` in `dd/MM/yyyy HH:mm:ss | comp | msg` form.
pub fn write_activity(component: &str, message: &str) {
    let dir = log_dir();
    fs::create_dir_all(&dir).ok();
    let line = format!("{} | {} | {}\n", timestamp_activity(), component, message);
    append(&dir.join("activity.log"), &line);
}

/// Rotates `debug.log` → `debug.1.log` → … once it exceeds `log_max_size_mb`.
fn maybe_rotate(cfg: &Config, dir: &Path) {
    let logf = dir.join("debug.log");
    let max_bytes = cfg.log_max_size_mb * 1024 * 1024;
    let Ok(meta) = fs::metadata(&logf) else {
        return;
    };
    if meta.len() <= max_bytes {
        return;
    }
    let max_files = cfg.log_max_files.max(1) as usize;
    // Shift debug.N.log → debug.N+1.log (dropping the oldest one).
    for i in (0..max_files).rev() {
        let src = dir.join(if i == 0 {
            "debug.log".to_string()
        } else {
            format!("debug.{i}.log")
        });
        if !src.exists() {
            continue;
        }
        let dst = dir.join(format!("debug.{}.log", i + 1));
        if i + 1 >= max_files {
            let _ = fs::remove_file(&src);
        } else {
            let _ = fs::rename(&src, &dst);
        }
    }
}

fn append(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn log_dir() -> PathBuf {
    config_home()
}

fn timestamp_log() -> String {
    // `%Y-%m-%d %H:%M:%S` without a chrono dependency.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Civil-date conversion of days since epoch (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

fn timestamp_activity() -> String {
    let t = timestamp_log();
    // Convert "yyyy-mm-dd HH:MM:SS" → "dd/mm/yyyy HH:MM:SS".
    let (date, time) = t.split_once(' ').unwrap_or(("", ""));
    let mut p = date.split('-');
    let (y, m, d) = (
        p.next().unwrap_or(""),
        p.next().unwrap_or(""),
        p.next().unwrap_or(""),
    );
    format!("{d}/{m}/{y} {time}")
}

fn level_name(l: Level) -> &'static str {
    match l {
        Level::Error => "error",
        Level::Info => "info",
        Level::Debug => "debug",
        Level::Trace => "trace",
    }
}

/// Formats a size in bytes → "1.5 GB" (divisor 1024, SI labels like the
/// original; the historical convention is kept despite being formally binary).
pub fn format_size(b: i64) -> String {
    if b <= 0 {
        return "0 B".to_string();
    }
    let mut v = b as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if v.abs() < 1024.0 {
            return format!("{v:.1} {unit}");
        }
        v /= 1024.0;
    }
    format!("{v:.1} PB")
}
