//! Configuration: load/save `~/.config/gdrive/settings.json`.
//!
//! Rust port of the relevant parts of `gdrive/config.py`. The full default
//! schema (mirroring `DEFAULTS`) powers the Synchronization and Settings pages;
//! typed getters/setters wrap the JSON object for ergonomic use.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

/// Returns the GDrive config directory (`~/.config/gdrive`).
pub fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME is not set");
    PathBuf::from(home).join(".config").join("gdrive")
}

/// Returns the settings file path (`~/.config/gdrive/settings.json`).
pub fn config_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// The default sync directory (`~/GoogleDrive`).
pub fn sync_dir_default() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME is not set");
    PathBuf::from(home).join("GoogleDrive")
}

/// Loads the config as a JSON object, merging defaults for missing keys.
/// Returns defaults when the file is absent or invalid.
pub fn load_config() -> HashMap<String, Value> {
    let Ok(text) = std::fs::read_to_string(config_file()) else {
        return defaults();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return defaults();
    };
    let Some(map) = value.as_object().cloned() else {
        return defaults();
    };
    let mut map: HashMap<String, Value> = map.into_iter().collect();
    for (k, v) in defaults() {
        map.entry(k).or_insert(v);
    }
    map
}

/// Persists the config object to `settings.json`.
pub fn save_config(map: &HashMap<String, Value>) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(config_file(), text);
    }
}

/// Reads a string field (empty string if absent).
pub fn get_string(map: &HashMap<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Writes a string field.
pub fn set_string(map: &mut HashMap<String, Value>, key: &str, val: &str) {
    map.insert(key.to_string(), Value::String(val.to_string()));
}

/// Reads a boolean field (false if absent).
pub fn get_bool(map: &HashMap<String, Value>, key: &str) -> bool {
    map.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Writes a boolean field.
pub fn set_bool(map: &mut HashMap<String, Value>, key: &str, val: bool) {
    map.insert(key.to_string(), Value::Bool(val));
}

/// Reads an integer field (0 if absent).
pub fn get_u64(map: &HashMap<String, Value>, key: &str) -> u64 {
    map.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

/// Writes an integer field.
pub fn set_u64(map: &mut HashMap<String, Value>, key: &str, val: u64) {
    map.insert(key.to_string(), Value::Number(val.into()));
}

/// Returns the list of selected folders (empty if unset).
pub fn selected_folders(map: &HashMap<String, Value>) -> Vec<String> {
    map.get("selected_folders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Sets the selected-folders list in the config map.
pub fn set_selected_folders(map: &mut HashMap<String, Value>, folders: &[String]) {
    let arr: Vec<Value> = folders.iter().map(|f| Value::String(f.clone())).collect();
    map.insert("selected_folders".to_string(), Value::Array(arr));
}

/// The default config, mirroring `DEFAULTS` in `gdrive/config.py`.
pub fn defaults() -> HashMap<String, Value> {
    let mut m = HashMap::new();
    set_string(
        &mut m,
        "sync_dir",
        &sync_dir_default().display().to_string(),
    );
    set_string(&mut m, "sync_interval", "manual");
    set_bool(&mut m, "watch_changes", false);
    set_bool(&mut m, "paused", false);
    set_string(&mut m, "bandwidth_limit", "");
    set_string(&mut m, "download_limit", "");
    set_string(&mut m, "upload_limit", "");
    set_selected_folders(&mut m, &[]);
    let excluded = [
        "*.tmp",
        "*.swp",
        ".DS_Store",
        "Thumbs.db",
        "~$*",
        "**/node_modules/**",
        "**/venv/**",
        "**/.venv/**",
        "**/__pycache__/**",
        "**/target/**",
    ];
    let arr: Vec<Value> = excluded
        .iter()
        .map(|s| Value::String(s.to_string()))
        .collect();
    m.insert("excluded_patterns".to_string(), Value::Array(arr));
    set_string(&mut m, "sync_mode", "mirror");
    set_bool(&mut m, "notifications_enabled", true);
    set_bool(&mut m, "launch_on_startup", true);
    set_bool(&mut m, "show_in_file_manager", true);
    set_bool(&mut m, "proxy_enabled", false);
    set_string(&mut m, "proxy_url", "");
    set_bool(&mut m, "encryption_enabled", false);
    set_bool(&mut m, "logging_enabled", true);
    set_string(&mut m, "log_level", "info");
    set_bool(&mut m, "log_rotate", true);
    set_u64(&mut m, "log_max_size_mb", 10);
    set_u64(&mut m, "log_max_files", 5);
    set_string(&mut m, "conflict_action", "newer");
    set_bool(&mut m, "remove_empty_dirs", false);
    set_bool(&mut m, "skip_hidden", true);
    set_bool(&mut m, "store_metadata", true);
    set_u64(&mut m, "num_threads", 8);
    set_string(&mut m, "language", "es");
    set_u64(&mut m, "window_width", 1000);
    set_u64(&mut m, "window_height", 700);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_folders_parses_array() {
        let mut m = HashMap::new();
        set_selected_folders(&mut m, &["a".into(), "b".into()]);
        assert_eq!(selected_folders(&m), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn selected_folders_empty_when_unset() {
        assert!(selected_folders(&HashMap::new()).is_empty());
    }

    #[test]
    fn typed_getters_default_when_missing() {
        let m = HashMap::new();
        assert_eq!(get_string(&m, "sync_mode"), "");
        assert!(!get_bool(&m, "paused"));
        assert_eq!(get_u64(&m, "num_threads"), 0);
    }

    #[test]
    fn defaults_cover_core_keys() {
        let d = defaults();
        assert_eq!(get_string(&d, "sync_mode"), "mirror");
        assert!(get_bool(&d, "skip_hidden"));
        assert_eq!(get_u64(&d, "num_threads"), 8);
    }
}
