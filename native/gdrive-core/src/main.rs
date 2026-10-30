//! Binary entry point of the core (for smoke tests / a minimal CLI).
//!
//! Useful to validate the toolchain and show the engine in action:
//!   gdrive-core about        → prints `rclone about` as JSON
//!   gdrive-core authorized   → prints `true`/`false` (valid token)
//!   gdrive-core email        → prints the email of the connected account

use std::env;
use std::process::ExitCode;

use gdrive_core::rclone::{rclone_about, rclone_is_authorized, rclone_user_email};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map_or("help", |s| s.as_str());

    match cmd {
        "about" | "storage" => match rclone_about() {
            Some(v) => {
                // `v` comes from an already parsed JSON; re-serializing with pretty
                // does not fail in practice. On the unexpected it degrades to `{}`.
                println!(
                    "{}",
                    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
                );
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("no se pudo obtener información de almacenamiento");
                ExitCode::FAILURE
            }
        },
        "authorized" => {
            println!("{}", rclone_is_authorized());
            ExitCode::SUCCESS
        }
        "email" => match rclone_user_email() {
            Some(e) => {
                println!("{e}");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("no se pudo obtener el email");
                ExitCode::FAILURE
            }
        },
        "version" => {
            println!("gdrive-core {}", gdrive_core::APP_VERSION);
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "uso: gdrive-core <about|authorized|email|version>\n\
                 versión: {}",
                gdrive_core::APP_VERSION
            );
            ExitCode::FAILURE
        }
    }
}
