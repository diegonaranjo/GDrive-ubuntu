//! Tipo de error unificado del crate.
//!
//! `gdrive-core` usa un único [`Error`] para todas las operaciones fallibles,
//! en lugar de `Result<T, String>`. Se implementa con [`thiserror`], de modo que
//! cada variante describe su causa con contexto y los callers pueden usar `?`
//! directamente. Es `Send + Sync + 'static`, por lo que es seguro propagarlo a
//! través de boundaries async (p. ej. tareas de Tokio).

use std::path::PathBuf;

/// Error del núcleo de GDrive.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error de entrada/salida del sistema de archivos.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// La configuración no se pudo (de)serializar.
    #[error("config JSON inválido: {0}")]
    Serde(#[from] serde_json::Error),

    /// `rclone` no pudo ejecutarse o devolvió un error.
    #[error("rclone: {0}")]
    Rclone(String),

    /// rclone dejó de emitir salida durante `SYNC_IDLE_TIMEOUT`.
    #[error("rclone sin actividad: se terminó el sync por estancamiento")]
    SyncTimeout,

    /// Un subproceso terminó con un código distinto de cero.
    #[error("subproceso salió con código {0}")]
    ProcessExit(String),

    /// No se pudo escribir la configuración.
    #[error("no se pudo persistir la configuración en {path}: {detail}")]
    SaveConfig {
        /// Ruta del archivo de configuración.
        path: PathBuf,
        /// Detalle de la causa del fallo.
        detail: String,
    },

    /// Un valor de configuración era inválido.
    #[error("valor de configuración inválido para '{key}': {value}")]
    InvalidConfig {
        /// Clave de configuración afectada.
        key: String,
        /// Valor inválido recibido.
        value: String,
    },
}

/// Alias de [`std::result::Result`] con el [`Error`] del crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
