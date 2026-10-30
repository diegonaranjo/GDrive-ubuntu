//! Logging con rotación + activity log. Reimplementa `log_write`/`write_activity`
//! de `gdrive/helpers.py` con una sola implementación canónica (el Python la
//! tenía triplicada y divergente).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{config_home, Config};

/// Nivel de severidad de un mensaje de log, ordenado de menor a mayor detalle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Error (el más prioritario).
    Error = 0,
    /// Información de funcionamiento normal.
    Info = 1,
    /// Detalle de depuración.
    Debug = 2,
    /// Trazado exhaustivo.
    Trace = 3,
}

impl FromStr for Level {
    type Err = std::convert::Infallible;

    /// Convierte una cadena (`"error"`, `"debug"`, `"trace"`) en un [`Level`].
    /// Cualquier valor no reconocido se interpreta como [`Level::Info`].
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
    /// Parsea el nivel desde una cadena con fallback `Info` (conveniencia sobre
    /// [`FromStr::from_str`]).
    pub fn parse(s: &str) -> Level {
        s.parse().unwrap_or(Level::Info)
    }
}

/// Escribe una línea en `debug.log` y, opcionalmente, en `activity.log`.
/// Filtra por nivel según `config.log_level`.
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

/// Anexa un evento al `activity.log` con formato `dd/MM/yyyy HH:mm:ss | comp | msg`.
pub fn write_activity(component: &str, message: &str) {
    let dir = log_dir();
    fs::create_dir_all(&dir).ok();
    let line = format!("{} | {} | {}\n", timestamp_activity(), component, message);
    append(&dir.join("activity.log"), &line);
}

/// Rota `debug.log` → `debug.1.log` → … cuando supera `log_max_size_mb`.
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
    // Desplazar debug.N.log → debug.N+1.log (eliminando el más viejo).
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
    // `%Y-%m-%d %H:%M:%S` sin dependencia de chrono.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Conversión civil de días desde epoch (algoritmo de Howard Hinnant).
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
    // Convertir "yyyy-mm-dd HH:MM:SS" → "dd/mm/yyyy HH:MM:SS".
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

/// Formatea un tamaño en bytes → "1.5 GB" (divisor 1024, etiquetas SI como el
/// original; se mantiene la convención histórica pese a ser formalmente binaria).
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
