//! Integration tests for the watcher (real inotify → debounce → channel flow).
//!
//! Unlike the unit tests in `src/watcher.rs` (which cover event filtering), here
//! the full flow is validated: watch a real directory, modify a file and receive
//! the signal after the debounce.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use gdrive_core::watcher::DirWatcher;

#[test]
fn watcher_signals_after_file_change() {
    let dir = tempfile::TempDir::new().unwrap();

    // `syncing=false`: no sync is running, so the watcher must react.
    let syncing = Arc::new(AtomicBool::new(false));
    let watcher = DirWatcher::new(dir.path().to_path_buf(), syncing)
        .spawn()
        .unwrap();

    // Give inotify time to register before touching the file.
    std::thread::sleep(Duration::from_millis(300));

    // Create a file: relevant event (Create).
    std::fs::write(dir.path().join("nuevo.txt"), b"hola").unwrap();

    // Wait for the signal (5s debounce + margin).
    let received = watcher.recv_timeout(Duration::from_secs(8));
    assert!(received.is_ok(), "no change signal was received");
}

#[test]
fn watcher_ignores_events_while_syncing() {
    let dir = tempfile::TempDir::new().unwrap();

    // `syncing=true`: the sync's own events are dropped (anti-loop).
    let syncing = Arc::new(AtomicBool::new(true));
    let watcher = DirWatcher::new(dir.path().to_path_buf(), syncing)
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(dir.path().join("nuevo.txt"), b"hola").unwrap();

    // It must not signal while `syncing` is active.
    let received = watcher.recv_timeout(Duration::from_secs(3));
    assert!(
        matches!(received, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
        "received an unexpected signal while a sync was running"
    );
}

#[test]
fn watcher_errors_when_dir_missing() {
    let missing = tempfile::TempDir::new().unwrap().path().join("no-existe");
    let syncing = Arc::new(AtomicBool::new(false));
    let watcher = DirWatcher::new(missing, syncing);
    assert!(watcher.spawn().is_err());
}
