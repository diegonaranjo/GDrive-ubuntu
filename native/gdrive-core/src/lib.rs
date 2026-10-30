//! GDrive Core — motor nativo (Rust) del cliente de escritorio Google Drive.
//!
//! Este crate reimplementa la lógica de orquestación que actualmente vive en
//! `gdrive/sync.py`, `gdrive/config.py`, `gdrive/helpers.py` e `gdrive/i18n.py`
//! del proyecto Python, con tipos seguros, `Result` en lugar de `None` mutantes
//! y tests reales (`cargo test`).
//!
//! El trabajo pesado (subir/bajar datos) lo sigue haciendo `rclone` como
//! subproceso; este crate sólo arma los comandos, gestiona el estado y parsea
//! la salida. La UI GTK4/libadwaita en Python se comunica con este núcleo vía
//! D-Bus (fase posterior).

//! Los errores del crate están unificados en [`Error`] (implementado con
//! `thiserror`); las funciones fallibles devuelven [`Result`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod config;
pub mod constants;
pub mod daemon;
pub mod error;
pub mod i18n;
pub mod logging;
pub mod rclone;
pub mod sync;
pub mod watcher;

pub use config::{config_file, config_home, load_config, save_config, Config};
pub use constants::{APP_ID, APP_VERSION, RCLONE_REMOTE};
pub use error::{Error, Result};
