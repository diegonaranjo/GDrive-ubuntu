//! Tests de integración del watcher (flujo real inotify → debounce → canal).
//!
//! A diferencia de los tests unitarios de `src/watcher.rs` (que cubren el
//! filtrado de eventos), aquí se valida el flujo completo: observar un directorio
//! real, modificar un archivo y recibir la señal tras el debounce.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use gdrive_core::watcher::DirWatcher;

#[test]
fn watcher_signals_after_file_change() {
    let dir = tempfile::TempDir::new().unwrap();

    // `syncing=false`: no hay sync en curso, así que el watcher debe reaccionar.
    let syncing = Arc::new(AtomicBool::new(false));
    let watcher = DirWatcher::new(dir.path().to_path_buf(), syncing)
        .spawn()
        .unwrap();

    // Dar tiempo a inotify para registrarse antes de tocar el archivo.
    std::thread::sleep(Duration::from_millis(300));

    // Crear un archivo: evento relevante (Create).
    std::fs::write(dir.path().join("nuevo.txt"), b"hola").unwrap();

    // Esperar la señal (debounce de 5s + margen).
    let received = watcher.recv_timeout(Duration::from_secs(8));
    assert!(received.is_ok(), "no se recibió señal de cambio");
}

#[test]
fn watcher_ignores_events_while_syncing() {
    let dir = tempfile::TempDir::new().unwrap();

    // `syncing=true`: los eventos del propio sync se descartan (anti-bucle).
    let syncing = Arc::new(AtomicBool::new(true));
    let watcher = DirWatcher::new(dir.path().to_path_buf(), syncing)
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(dir.path().join("nuevo.txt"), b"hola").unwrap();

    // No debe señalizar mientras `syncing` esté activo.
    let received = watcher.recv_timeout(Duration::from_secs(3));
    assert!(
        matches!(received, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
        "recibió señal inesperada durante un sync en curso"
    );
}

#[test]
fn watcher_errors_when_dir_missing() {
    let missing = tempfile::TempDir::new().unwrap().path().join("no-existe");
    let syncing = Arc::new(AtomicBool::new(false));
    let watcher = DirWatcher::new(missing, syncing);
    assert!(watcher.spawn().is_err());
}
