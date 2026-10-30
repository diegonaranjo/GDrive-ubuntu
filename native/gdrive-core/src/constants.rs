//! Constantes centralizadas de la aplicación.

use std::path::PathBuf;

/// Application ID de D-Bus (compartido con la UI GTK).
pub const APP_ID: &str = "com.gdrive.Client";
/// Nombre legible de la aplicación.
pub const APP_NAME: &str = "Google Drive";
/// Versión semántica de la aplicación.
pub const APP_VERSION: &str = "1.1.0";

/// Remote de rclone. Respeta la variable de entorno `GDRIVE_REMOTE` como el
/// original en Python, por compatibilidad.
pub fn rclone_remote() -> String {
    match std::env::var("GDRIVE_REMOTE") {
        Ok(v) if !v.is_empty() => v,
        _ => "gdrive:".to_string(),
    }
}

/// Constante de conveniencia (compatible con el código Python que usaba la
/// variable global `RCLONE_REMOTE`).
pub const RCLONE_REMOTE: &str = "gdrive:";

/// Carpeta de sincronización por defecto: `$HOME/GoogleDrive`.
pub fn sync_dir_default() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join("GoogleDrive"),
        None => PathBuf::from("GoogleDrive"),
    }
}

/// Color de cada estado (identidad visual Google Drive).
pub const COLORS: [(&str, &str); 4] = [
    ("idle", "#1E8E3E"),
    ("syncing", "#1A73E8"),
    ("paused", "#F9AB00"),
    ("error", "#D93025"),
];

/// Directorio del registro de actividad (cache/config).
pub fn cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(c) if !c.is_empty() => PathBuf::from(c),
        _ => match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(".cache"),
            None => PathBuf::from(".cache"),
        },
    }
}

/// Ruta del directorio de listas/locks de rclone bisync.
pub fn bisync_cache_dir() -> PathBuf {
    cache_dir().join("rclone").join("bisync")
}
