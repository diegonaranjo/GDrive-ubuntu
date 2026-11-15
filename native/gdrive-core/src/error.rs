//! Unified error type for the crate.
//!
//! `gdrive-core` uses a single [`Error`] for all fallible operations instead of
//! `Result<T, String>`. It is implemented with [`thiserror`], so each variant
//! describes its cause with context and callers can use `?` directly. It is
//! `Send + Sync + 'static`, so it is safe to propagate it across async
//! boundaries (e.g. Tokio tasks).

use std::path::PathBuf;

use crate::i18n;

/// Error of the GDrive core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// File-system input/output error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The configuration could not be (de)serialized.
    ///
    /// `Display` is developer-facing English; [`Error::message`] resolves the
    /// text through the i18n catalogs.
    #[error("invalid config JSON: {0}")]
    Serde(#[from] serde_json::Error),

    /// `rclone` could not be run or returned an error.
    #[error("rclone: {0}")]
    Rclone(String),

    /// rclone stopped emitting output for `SYNC_IDLE_TIMEOUT`.
    #[error("rclone went idle: sync aborted after stalling")]
    SyncTimeout,

    /// A subprocess ended with a non-zero exit code.
    #[error("subprocess exited with code {0}")]
    ProcessExit(String),

    /// The configuration could not be written.
    #[error("could not persist the configuration to {path}: {detail}")]
    SaveConfig {
        /// Path of the configuration file.
        path: PathBuf,
        /// Detail of the cause of the failure.
        detail: String,
    },

    /// A configuration value was invalid.
    #[error("invalid configuration value for '{key}': {value}")]
    InvalidConfig {
        /// Configuration key affected.
        key: String,
        /// Invalid value received.
        value: String,
    },
}

impl Error {
    /// User-facing, localized text for this error.
    ///
    /// `Display` stays developer-facing English so logs and tests do not depend
    /// on the active language. Anything shown in the window or sent over D-Bus
    /// must use this method instead: `thiserror` formats statically, so the
    /// translation has to be resolved at call time.
    ///
    /// Variants that only wrap an OS or subprocess detail keep that detail
    /// verbatim; there is nothing translatable in it.
    pub fn message(&self) -> String {
        match self {
            Error::Serde(e) => i18n::t_args("error.serde", &[&e.to_string()]),
            Error::SyncTimeout => i18n::t("error.sync_timeout"),
            Error::ProcessExit(code) => i18n::t_args("error.process_exit", &[code]),
            Error::SaveConfig { path, detail } => {
                i18n::t_args("error.save_config", &[&path.display().to_string(), detail])
            }
            Error::InvalidConfig { key, value } => {
                i18n::t_args("error.invalid_config", &[key, value])
            }
            Error::Io(_) | Error::Rclone(_) => self.to_string(),
        }
    }
}

/// Alias of [`std::result::Result`] with the crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
