//! Entry-point binario del core (para smoke test / CLI mínima).
//!
//! Útil para validar el toolchain y demostrar el motor en acción:
//!   gdrive-core about        → imprime `rclone about` en JSON
//!   gdrive-core authorized   → imprime `true`/`false` (token válido)
//!   gdrive-core email        → imprime el email de la cuenta conectada

use std::env;
use std::process::ExitCode;

use gdrive_core::rclone::{rclone_about, rclone_is_authorized, rclone_user_email};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map_or("help", |s| s.as_str());

    match cmd {
        "about" | "storage" => match rclone_about() {
            Some(v) => {
                // `v` proviene de un JSON ya parseado; re-serializar con pretty no
                // falla en la práctica. Ante lo inesperado se degrada a `{}`.
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
