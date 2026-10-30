//! Configuración: defaults tipados, carga y guardado (JSON en ~/.config/gdrive/settings.json).
//!
//! Reemplaza a `gdrive/config.py`. La diferencia clave frente al Python: es un
//! struct tipado con `serde`, de modo que un valor inválido se detecta en carga
//! en vez de filtrarse silenciosamente.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::constants::sync_dir_default;
use crate::error::{Error, Result};

/// Subdirectorio de configuración (relativo a `$HOME`).
pub const CONFIG_DIR: &str = ".config/gdrive";

/// Configuración completa. Los valores por defecto se aplican al deserializar
/// (los campos ausentes usan `#[serde(default)]`), de modo que un archivo viejo
/// siempre se completa con defaults actuales.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Carpeta de sincronización local (ruta absoluta o relativa).
    pub sync_dir: String,
    /// Intervalo de sincronización automática (`manual`, `5min`, `15min`, `30min`, `1hr`).
    pub sync_interval: String,
    /// `true` si la detección de cambios en tiempo real (watcher inotify en
    /// `sync_dir`) está habilitada. Dispara un sync al detectar cambios locales.
    /// Desactivada por defecto: la sync automática requiere consentimiento.
    pub watch_changes: bool,
    /// `true` si la sincronización está pausada por el usuario.
    pub paused: bool,
    /// Límite de ancho de banda global de rclone (cadena vacía = sin límite).
    pub bandwidth_limit: String,
    /// Límite de ancho de banda de descarga (cadena vacía = sin límite).
    pub download_limit: String,
    /// Límite de ancho de banda de subida (cadena vacía = sin límite).
    pub upload_limit: String,
    /// Carpetas seleccionadas para sincronización selectiva.
    pub selected_folders: Vec<String>,
    /// Patrones glob excluidos del sync.
    pub excluded_patterns: Vec<String>,
    /// Modo de sincronización (`mirror`, `stream`, `selective`).
    pub sync_mode: String,
    /// `true` si las notificaciones de escritorio están habilitadas.
    pub notifications_enabled: bool,
    /// `true` si la app debe iniciarse al arrancar sesión.
    pub launch_on_startup: bool,
    /// `true` si se muestra la carpeta en el gestor de archivos.
    pub show_in_file_manager: bool,
    /// `true` si se usa proxy para rclone.
    pub proxy_enabled: bool,
    /// URL del proxy (p. ej. `socks5://127.0.0.1:1080`).
    pub proxy_url: String,
    /// `true` si el cifrado está habilitado.
    pub encryption_enabled: bool,
    /// `true` si el logging está habilitado.
    pub logging_enabled: bool,
    /// Nivel de log (`error`, `info`, `debug`, `trace`).
    pub log_level: String,
    /// `true` si se rotan los archivos de log por tamaño.
    pub log_rotate: bool,
    /// Tamaño máximo por archivo de log (en MB) antes de rotar.
    pub log_max_size_mb: u64,
    /// Número máximo de archivos de log rotados a conservar.
    pub log_max_files: u32,
    /// Resolución de conflictos pasada a `--conflict-resolve` (`newer`, etc.).
    pub conflict_action: String,
    /// `true` pasa `--remove-empty-dirs`: quita directorios vacíos tras el sync.
    pub remove_empty_dirs: bool,
    /// `true` si se excluyen archivos ocultos (`.git`, `.*`).
    pub skip_hidden: bool,
    /// `true` pasa `--metadata` (guarda metadatos; no la fecha de modificación).
    pub store_metadata: bool,
    /// Número de transferencias concurrentes (hilos de rclone).
    pub num_threads: u32,
    /// Idioma de la interfaz (`es`, `en`).
    pub language: String,
    /// Ancho de la ventana principal (px).
    pub window_width: i32,
    /// Alto de la ventana principal (px).
    pub window_height: i32,
    /// Timestamp del último sync (se escribe en runtime; no tiene default útil).
    pub last_sync: String,
    /// `true` si el último sync terminó con éxito.
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
    /// Acceso compatible con `config.get("clave", default)` de Python.
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

    /// `get_bool("clave")` para flags booleanos.
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

/// Directorio de configuración resuelto (respeta `GDRIVE_CONFIG_DIR` para tests).
pub fn config_home() -> PathBuf {
    match std::env::var_os("GDRIVE_CONFIG_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(CONFIG_DIR),
            None => PathBuf::from(CONFIG_DIR),
        },
    }
}

/// Ruta completa del archivo de configuración (`settings.json`).
pub fn config_file() -> PathBuf {
    config_home().join("settings.json")
}

/// Carga la configuración del disco, aplicando defaults a campos ausentes.
/// Devuelve un error explícito si el JSON es inválido (a diferencia del Python
/// que silenciosamente devolvía defaults ante cualquier excepción).
pub fn load_config() -> Result<Config> {
    let f = config_file();
    if !f.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&f)?;
    let cfg = serde_json::from_str(&raw)?;
    Ok(cfg)
}

/// Carga con fallback a defaults (comportamiento 100% compatible con el Python:
/// nunca falla, ante cualquier error devuelve la config por defecto).
pub fn load_config_or_default() -> Config {
    load_config().unwrap_or_default()
}

/// Guarda la configuración de forma atómica (escribe a temp y renombra),
/// evitando corromper el archivo si el proceso muere a mitad de escritura.
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

/// Devuelve `true` si el archivo de configuración existe.
pub fn config_exists() -> bool {
    config_file().exists()
}

/// Elimina el archivo de configuración (restablecer a defaults).
pub fn reset_config() -> Result<()> {
    let path = config_file();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
