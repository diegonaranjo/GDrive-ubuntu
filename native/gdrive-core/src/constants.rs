//! Centralized application constants.

use std::path::PathBuf;

/// D-Bus application ID (shared with the GTK UI).
pub const APP_ID: &str = "com.gdrive.Client";
/// Human-readable application name.
pub const APP_NAME: &str = "Google Drive";
/// Semantic version of the application.
pub const APP_VERSION: &str = "1.1.0";

/// rclone remote. Honors the `GDRIVE_REMOTE` environment variable like the
/// original Python, for compatibility.
pub fn rclone_remote() -> String {
    match std::env::var("GDRIVE_REMOTE") {
        Ok(v) if !v.is_empty() => v,
        _ => "gdrive:".to_string(),
    }
}

/// Convenience constant (compatible with the Python code that used the
/// `RCLONE_REMOTE` global variable).
pub const RCLONE_REMOTE: &str = "gdrive:";

/// Default synchronization folder: `$HOME/GoogleDrive`.
pub fn sync_dir_default() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join("GoogleDrive"),
        None => PathBuf::from("GoogleDrive"),
    }
}

/// Color of each state (Google Drive visual identity).
pub const COLORS: [(&str, &str); 4] = [
    ("idle", "#1E8E3E"),
    ("syncing", "#1A73E8"),
    ("paused", "#F9AB00"),
    ("error", "#D93025"),
];

/// Directory of the activity log (cache/config).
pub fn cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(c) if !c.is_empty() => PathBuf::from(c),
        _ => match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(".cache"),
            None => PathBuf::from(".cache"),
        },
    }
}

/// Path of the rclone bisync lists/locks directory.
pub fn bisync_cache_dir() -> PathBuf {
    cache_dir().join("rclone").join("bisync")
}
