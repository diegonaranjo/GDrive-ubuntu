//! Unified error type for the crate.
//!
//! `gdrive-core` uses a single [`Error`] for all fallible operations instead of
//! `Result<T, String>`. It is implemented with [`thiserror`], so each variant
//! describes its cause with context and callers can use `?` directly. It is
//! `Send + Sync + 'static`, so it is safe to propagate it across async
//! boundaries (e.g. Tokio tasks).

use std::path::PathBuf;

/// Error of the GDrive core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// File-system input/output error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The configuration could not be (de)serialized.
    #[error("config JSON inválido: {0}")]
    Serde(#[from] serde_json::Error),

    /// `rclone` could not be run or returned an error.
    #[error("rclone: {0}")]
    Rclone(String),

    /// rclone stopped emitting output for `SYNC_IDLE_TIMEOUT`.
    #[error("rclone sin actividad: se terminó el sync por estancamiento")]
    SyncTimeout,

    /// A subprocess ended with a non-zero exit code.
    #[error("subproceso salió con código {0}")]
    ProcessExit(String),

    /// The configuration could not be written.
    #[error("no se pudo persistir la configuración en {path}: {detail}")]
    SaveConfig {
        /// Path of the configuration file.
        path: PathBuf,
        /// Detail of the cause of the failure.
        detail: String,
    },

    /// A configuration value was invalid.
    #[error("valor de configuración inválido para '{key}': {value}")]
    InvalidConfig {
        /// Configuration key affected.
        key: String,
        /// Invalid value received.
        value: String,
    },
}

/// Alias of [`std::result::Result`] with the crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
