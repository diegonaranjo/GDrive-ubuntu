//! Utility helpers: size formatting and `rclone` queries.
//!
//! Rust port of the relevant parts of `gdrive/helpers.py`. These are pure,
//! blocking functions (they spawn `rclone` as a subprocess); callers run them
//! off the GTK main loop and marshal results back with `glib::idle_add`.

use std::process::Command;

/// The rclone remote used by this client (mirrors `RCLONE_REMOTE`).
pub const RCLONE_REMOTE: &str = "gdrive:";

/// Formats a byte count into a compact human-readable string (`0 B`, `1.5 GB`).
pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if value < 1024.0 {
            return format!("{value:.1} {unit}");
        }
        value /= 1024.0;
    }
    format!("{value:.1} PB")
}

/// Runs `rclone about <remote> --json` and returns the parsed JSON object, or
/// `None` on failure.
pub fn rclone_about() -> Option<serde_json::Value> {
    let out = Command::new("rclone")
        .args(["about", RCLONE_REMOTE, "--json"])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Resolves the email of the account linked to the remote, or `None`.
pub fn rclone_user_email() -> Option<String> {
    // 1) Try reading a user/account/email field from the rclone config.
    if let Ok(out) = Command::new("rclone")
        .args(["config", "show", RCLONE_REMOTE])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let line = line.trim();
                for key in ["user", "account", "email"] {
                    if let Some(rest) = line.strip_prefix(key).and_then(|r| r.strip_prefix(" =")) {
                        let val = rest.trim();
                        if !val.is_empty() {
                            return Some(val.to_string());
                        }
                    }
                }
            }
        }
    }
    // 2) Google Drive: query the backend's own API.
    let out = Command::new("rclone")
        .args(["backend", "user", RCLONE_REMOTE])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let data: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    data.get("email")
        .or_else(|| data.get("userEmailAddress"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Lists top-level directories of the remote via `rclone lsd`.
pub fn rclone_lsd(remote: &str) -> Vec<String> {
    let Ok(out) = Command::new("rclone").args(["lsd", remote]).output() else {
        return Vec::new();
    };
    if !out.status.success() || out.stdout.is_empty() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(|s| s.to_string())
        .collect()
}

/// Opens a URL in the default browser via `xdg-open`.
pub fn open_url(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500.0 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(15 * 1024 * 1024 * 1024), "15.0 GB");
    }

    #[test]
    fn format_size_fractional() {
        assert_eq!(format_size(1536), "1.5 KB");
    }
}
