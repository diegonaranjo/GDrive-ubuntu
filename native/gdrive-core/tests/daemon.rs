//! Tests del daemon (lógica pura, sin requerir bus de sesión).

#![cfg(test)]

use std::sync::atomic::Ordering;

use gdrive_core::config::Config;
use gdrive_core::daemon::{service_details, SharedState};
use gdrive_core::sync::SyncStatus;

#[test]
fn test_service_details() {
    let (name, path) = service_details();
    assert_eq!(name, "com.gdrive.Client");
    assert_eq!(path, "/com/gdrive/Client");
}

#[test]
fn test_shared_state_initial() {
    let state = SharedState::new(Config::default());
    assert_eq!(state.current_status(), SyncStatus::Idle);
    assert!(!state.is_paused());
    assert!(!state.quit_requested.load(Ordering::SeqCst));
    // Un sync no está en curso inicialmente (flag syncing = false).
    assert!(!state.syncing.load(Ordering::SeqCst));
}

#[test]
fn test_shared_state_request_quit() {
    let state = SharedState::new(Config::default());
    assert!(!state.quit_requested.load(Ordering::SeqCst));
    state.request_quit();
    assert!(state.quit_requested.load(Ordering::SeqCst));
}

#[test]
fn test_shared_state_paused_flag() {
    let cfg = Config {
        paused: true,
        ..Config::default()
    };
    let state = SharedState::new(cfg);
    assert!(state.is_paused());
}
