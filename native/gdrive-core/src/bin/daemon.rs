//! Entry-point of GDrive's D-Bus daemon.
//!
//! Usage:
//!   gdrive-daemon            → runs in the foreground (for debugging)
//!   gdrive-daemon --once     → runs one manual sync and exits
//!
//! The daemon registers the name `com.gdrive.Client` (compatible with the
//! existing tray that runs `gapplication action com.gdrive.Client <name>`).

use std::time::Duration;

use gdrive_core::config::load_config_or_default;
use gdrive_core::daemon::{
    build_server, spawn_change_watcher, spawn_periodic_scheduler, SharedState,
};
use gdrive_core::i18n;
use gdrive_core::logging::{log_write, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config_or_default();
    i18n::set_language(&config.language);

    let args: Vec<String> = std::env::args().collect();
    let once = args.iter().any(|a| a == "--once");

    let state = SharedState::new(config);

    if once {
        let engine = gdrive_core::sync::SyncEngine::new(state.config.lock().unwrap().clone());
        match engine.run() {
            Ok(o) => {
                println!("{}", o.message);
                log_write(
                    &state.config.lock().unwrap(),
                    Level::Info,
                    "Daemon",
                    &o.message,
                    true,
                );
            }
            Err(e) => {
                eprintln!("{e}");
                log_write(
                    &state.config.lock().unwrap(),
                    Level::Error,
                    "Daemon",
                    &e.to_string(),
                    true,
                );
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    log_write(
        &state.config.lock().unwrap(),
        Level::Info,
        "Daemon",
        "starting gdrive-daemon",
        false,
    );

    // If the name can't be acquired, another daemon already serves the service: this
    // process must exit instead of being queued with its scheduler active.
    let _conn = match build_server(state.clone()).await {
        Ok(conn) => conn,
        Err(e) => {
            log_write(
                &state.config.lock().unwrap(),
                Level::Error,
                "Daemon",
                &format!(
                    "otro daemon ya posee {}; este proceso sale ({e})",
                    gdrive_core::APP_ID
                ),
                true,
            );
            return Err(e.into());
        }
    };

    // Periodic sync scheduler (no-op if sync_interval == "manual").
    spawn_periodic_scheduler(state.clone());

    // Real-time change detection (no-op if watch_changes == false).
    spawn_change_watcher(state.clone());

    // Main loop: wait for SIGINT/SIGTERM or the quit command via D-Bus.
    loop {
        if state
            .quit_requested
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            log_write(
                &state.config.lock().unwrap(),
                Level::Info,
                "Daemon",
                "stopping (quit)",
                false,
            );
            break;
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log_write(&state.config.lock().unwrap(), Level::Info, "Daemon", "stopping (SIGINT)", false);
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
    }

    Ok(())
}
