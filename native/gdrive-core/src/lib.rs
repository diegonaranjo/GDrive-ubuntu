//! GDrive Core — native (Rust) engine of the Google Drive desktop client.
//!
//! This crate reimplements the orchestration logic that lived in the Python
//! project's `gdrive/config.py`, `gdrive/helpers.py` and `gdrive/i18n.py` (and
//! its legacy `gdrive/sync.py`, removed in 1.1.0), with safe types, `Result`
//! instead of mutating `None` and real tests (`cargo test`).
//!
//! The heavy work (uploading/downloading data) is still done by `rclone` as a
//! subprocess; this crate only assembles the commands, manages the state and
//! parses the output. The Python GTK4/libadwaita UI talks to this core over
//! D-Bus (later phase).

//! Crate errors are unified in [`Error`] (implemented with `thiserror`);
//! fallible functions return [`Result`].

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
