//! Configuration: typed defaults, loading and saving (JSON at ~/.config/gdrive/settings.json).
//!
//! Replaces `gdrive/config.py`. The key difference vs the Python version: it's a
//! typed struct with `serde`, so an invalid value is detected at load
//! time instead of silently leaking through.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::constants::sync_dir_default;
use crate::error::{Error, Result};

/// Configuration subdirectory (relative to `$HOME`).
pub const CONFIG_DIR: &str = ".config/gdrive";

/// Complete configuration. Default values are applied when deserializing
/// (missing fields use `#[serde(default)]`), so an old file
/// is always completed with current defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Local synchronization folder (absolute or relative path).
    pub sync_dir: String,
    /// Automatic sync interval (`manual`, `5min`, `15min`, `30min`, `1hr`).
    pub sync_interval: String,
    /// `true` if real-time change detection (inotify watcher on
    /// `sync_dir`) is enabled. Triggers a sync when local changes are detected.
    /// Disabled by default: automatic sync requires consent.
    pub watch_changes: bool,
    /// `true` if synchronization is paused by the user.
    pub paused: bool,
    /// Global rclone bandwidth limit (empty string = no limit).
    pub bandwidth_limit: String,
    /// Download bandwidth limit (empty string = no limit).
    pub download_limit: String,
    /// Upload bandwidth limit (empty string = no limit).
    pub upload_limit: String,
    /// Folders selected for selective synchronization.
    pub selected_folders: Vec<String>,
    /// Glob patterns excluded from the sync.
    pub excluded_patterns: Vec<String>,
    /// Synchronization mode (`mirror`, `stream`, `selective`).
    pub sync_mode: String,
    /// `true` if desktop notifications are enabled.
    pub notifications_enabled: bool,
    /// `true` if the app should start at session login.
    pub launch_on_startup: bool,
    /// `true` if the folder is shown in the file manager.
    pub show_in_file_manager: bool,
    /// `true` if a proxy is used for rclone.
    pub proxy_enabled: bool,
    /// Proxy URL (e.g. `socks5://127.0.0.1:1080`).
    pub proxy_url: String,
    /// `true` if encryption is enabled.
    pub encryption_enabled: bool,
    /// `true` if logging is enabled.
    pub logging_enabled: bool,
    /// Log level (`error`, `info`, `debug`, `trace`).
    pub log_level: String,
    /// `true` if log files are rotated by size.
    pub log_rotate: bool,
    /// Maximum size per log file (in MB) before rotating.
    pub log_max_size_mb: u64,
    /// Maximum number of rotated log files to keep.
    pub log_max_files: u32,
    /// Conflict resolution passed to `--conflict-resolve` (`newer`, etc.).
    pub conflict_action: String,
    /// `true` passes `--remove-empty-dirs`: removes empty directories after the sync.
    pub remove_empty_dirs: bool,
    /// `true` if hidden files are excluded (`.git`, `.*`).
    pub skip_hidden: bool,
    /// `true` passes `--metadata` (stores metadata; not the modification date).
    pub store_metadata: bool,
    /// Number of concurrent transfers (rclone threads).
    pub num_threads: u32,
    /// Interface language (`es`, `en`).
    pub language: String,
    /// Width of the main window (px).
    pub window_width: i32,
    /// Height of the main window (px).
    pub window_height: i32,
    /// Timestamp of the last sync (written at runtime; has no useful default).
    pub last_sync: String,
    /// `true` if the last sync finished successfully.
    pub last_sync_ok: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sync_dir: sync_dir_default().to_string_lossy().to_string(),
            sync_interval: "manual".into(),
            watch_changes: false,
            paused: false,
            bandwidth_limit: String::new(),
            download_limit: String::new(),
            upload_limit: String::new(),
            selected_folders: vec![],
            excluded_patterns: vec![
                "*.tmp".into(),
                "*.swp".into(),
                ".DS_Store".into(),
                "Thumbs.db".into(),
                "~$*".into(),
                "**/node_modules/**".into(),
                "**/venv/**".into(),
                "**/.venv/**".into(),
                "**/__pycache__/**".into(),
                "**/target/**".into(),
            ],
            sync_mode: "mirror".into(),
            notifications_enabled: true,
            launch_on_startup: true,
            show_in_file_manager: true,
            proxy_enabled: false,
            proxy_url: String::new(),
            encryption_enabled: false,
            logging_enabled: true,
            log_level: "info".into(),
            log_rotate: true,
            log_max_size_mb: 10,
            log_max_files: 5,
            conflict_action: "newer".into(),
            remove_empty_dirs: false,
            skip_hidden: true,
            store_metadata: true,
            num_threads: 8,
            language: "es".into(),
            window_width: 1000,
            window_height: 700,
            last_sync: String::new(),
            last_sync_ok: false,
        }
    }
}

impl Config {
    /// Python-compatible accessor for `config.get("key", default)`.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        match key {
            "sync_dir" => Some(&self.sync_dir),
            "sync_interval" => Some(&self.sync_interval),
            "bandwidth_limit" => Some(&self.bandwidth_limit),
            "download_limit" => Some(&self.download_limit),
            "upload_limit" => Some(&self.upload_limit),
            "sync_mode" => Some(&self.sync_mode),
            "proxy_url" => Some(&self.proxy_url),
            "log_level" => Some(&self.log_level),
            "conflict_action" => Some(&self.conflict_action),
            "language" => Some(&self.language),
            "last_sync" => Some(&self.last_sync),
            _ => None,
        }
    }

    /// `get_bool("key")` for boolean flags.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match key {
            "paused" => Some(self.paused),
            "notifications_enabled" => Some(self.notifications_enabled),
            "launch_on_startup" => Some(self.launch_on_startup),
            "watch_changes" => Some(self.watch_changes),
            "show_in_file_manager" => Some(self.show_in_file_manager),
            "proxy_enabled" => Some(self.proxy_enabled),
            "encryption_enabled" => Some(self.encryption_enabled),
            "logging_enabled" => Some(self.logging_enabled),
            "log_rotate" => Some(self.log_rotate),
            "remove_empty_dirs" => Some(self.remove_empty_dirs),
            "skip_hidden" => Some(self.skip_hidden),
            "store_metadata" => Some(self.store_metadata),
            "last_sync_ok" => Some(self.last_sync_ok),
            _ => None,
        }
    }
}

/// Resolved configuration directory (honors `GDRIVE_CONFIG_DIR` for tests).
pub fn config_home() -> PathBuf {
    match std::env::var_os("GDRIVE_CONFIG_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(CONFIG_DIR),
            None => PathBuf::from(CONFIG_DIR),
        },
    }
}

/// Full path of the configuration file (`settings.json`).
pub fn config_file() -> PathBuf {
    config_home().join("settings.json")
}

/// Loads the configuration from disk, applying defaults to missing fields.
/// Returns an explicit error if the JSON is invalid (unlike the Python
/// version, which silently returned defaults on any exception).
pub fn load_config() -> Result<Config> {
    let f = config_file();
    if !f.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&f)?;
    let cfg = serde_json::from_str(&raw)?;
    Ok(cfg)
}

/// Loads with fallback to defaults (100% Python-compatible behavior:
/// never fails; on any error it returns the default config).
pub fn load_config_or_default() -> Config {
    load_config().unwrap_or_default()
}

/// Saves the configuration atomically (writes to a temp file and renames),
/// avoiding corrupting the file if the process dies mid-write.
pub fn save_config(cfg: &Config) -> Result<()> {
    let dir = config_home();
    std::fs::create_dir_all(&dir)?;
    let path = config_file();
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &path).map_err(|e| Error::SaveConfig {
        path,
        detail: e.to_string(),
    })?;
    Ok(())
}

/// Returns `true` if the configuration file exists.
pub fn config_exists() -> bool {
    config_file().exists()
}

/// Deletes the configuration file (reset to defaults).
pub fn reset_config() -> Result<()> {
    let path = config_file();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
