//! Logging with rotation + activity log. Reimplements `log_write` and
//! `write_activity` from `gdrive/helpers.py` as a single canonical
//! implementation (the Python had it triplicated and divergent).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::Local;

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
        write_activity(cfg, component, message);
    }
}

/// Longest single activity entry kept: the full text always goes to
/// `debug.log`, and one legacy sync run once put a 23 MB multi-line rclone dump
/// into `activity.log`, which the Activity page then read whole.
const ACTIVITY_MESSAGE_MAX_CHARS: usize = 400;

/// Appends ONE line to `activity.log` in `dd/MM/yyyy HH:mm:ss | comp | msg` form.
/// Embedded newlines are flattened and the message is capped, so an entry can
/// never grow past one line no matter what rclone printed.
pub fn write_activity(cfg: &Config, component: &str, message: &str) {
    let dir = log_dir();
    fs::create_dir_all(&dir).ok();
    // Rotate before writing, like debug.log: rotating after would leave no
    // activity.log at all until the next event.
    if cfg.log_rotate {
        rotate(
            &dir,
            "activity.log",
            cfg.log_max_size_mb * 1024 * 1024,
            cfg.log_max_files.max(1) as usize,
        );
    }
    let flat = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = if flat.chars().count() > ACTIVITY_MESSAGE_MAX_CHARS {
        let kept: String = flat.chars().take(ACTIVITY_MESSAGE_MAX_CHARS).collect();
        format!("{kept}… (texto completo en debug.log)")
    } else {
        flat
    };
    let line = format!("{} | {} | {}\n", timestamp_activity(), component, flat);
    append(&dir.join("activity.log"), &line);
}

/// Rotates `debug.log` → `debug.1.log` → … once it exceeds `log_max_size_mb`.
fn maybe_rotate(cfg: &Config, dir: &Path) {
    rotate(
        dir,
        "debug.log",
        cfg.log_max_size_mb * 1024 * 1024,
        cfg.log_max_files.max(1) as usize,
    );
}

/// Shifts `base` → `base.1.log` → … once `base` exceeds `max_bytes`, keeping at
/// most `max_files` generations. Used for both logs, and parameterized in bytes
/// so a test can exercise it without writing a megabyte.
fn rotate(dir: &Path, base: &str, max_bytes: u64, max_files: usize) {
    let stem = base.strip_suffix(".log").unwrap_or(base);
    let logf = dir.join(base);
    let Ok(meta) = fs::metadata(&logf) else {
        return;
    };
    if meta.len() <= max_bytes {
        return;
    }
    for i in (0..max_files).rev() {
        let src = dir.join(if i == 0 {
            base.to_string()
        } else {
            format!("{stem}.{i}.log")
        });
        if !src.exists() {
            continue;
        }
        let dst = dir.join(format!("{stem}.{}.log", i + 1));
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

/// `%Y-%m-%d %H:%M:%S` in the machine's local time, matching what the Python
/// side writes (`time.strftime`, also local) and the timestamps rclone prints
/// in the lines this file embeds. Deriving it from `SystemTime` alone yields
/// UTC, which put this log hours ahead of the rest on a host west of UTC.
fn timestamp_log() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn timestamp_activity() -> String {
    // `%d/%m/%Y %H:%M:%S` — the flat layout activity.log has always used.
    Local::now().format("%d/%m/%Y %H:%M:%S").to_string()
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
