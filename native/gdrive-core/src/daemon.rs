//! GDrive's D-Bus daemon (Phase 2).
//!
//! Exposes the engine (`SyncEngine`) to the Python GTK4 UI over D-Bus. Serves the
//! bus name `com.gdrive.Client` (compatible with the current tray icon, which
//! runs `gapplication action com.gdrive.Client <name>` via `org.gtk.Actions`).
//!
//! Main interface (for the UI): `com.gdrive.Daemon1`
//!   - `SyncNow() -> s`
//!   - `Pause()`, `Resume()`
//!   - `GetStatus() -> a{ss}`   (map: status, paused, version, ...)
//!   - `Quit()`
//!   - `StatusChanged(s status, b paused)` signal
//!
//! The sync runs in `tokio::task::spawn_blocking` (rclone is a blocking
//! subprocess), so the reactor never blocks.

//! The `zbus` `#[interface]` macro generates the D-Bus interfaces' "glue" as
//! public items without documentation, triggering false positives from
//! `missing_docs` on code that is not ours. The real documentation of
//! each method lives in its `///` comments (which zbus uses for the interface
//! XML). The lint is disabled here, not crate-wide.
#![allow(missing_docs)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::interface;
use zbus::zvariant::OwnedValue;

use crate::config::{load_config_or_default, save_config, Config};
use crate::constants::{APP_ID, APP_VERSION};
use crate::logging::{log_write, Level};
use crate::sync::{interval_seconds, SyncEngine, SyncStatus};
use crate::watcher::DirWatcher;

/// Shared state: reactor ↔ sync thread.
#[derive(Debug)]
pub struct SharedState {
    /// Current configuration (mutex-protected for multi-threaded access).
    pub config: Mutex<Config>,
    /// Current synchronization status.
    pub status: Mutex<SyncStatus>,
    /// `true` while a sync is in progress. `Arc` to share it, cloned, with
    /// the change watcher (which discards events during a sync).
    pub syncing: Arc<AtomicBool>,
    /// `true` when daemon shutdown has been requested.
    pub quit_requested: AtomicBool,
    /// Current progress `(transferred, total)` of the in-flight sync.
    pub progress: Mutex<(u64, u64)>,
    /// Current (formatted) speed of the in-flight sync.
    pub speed: Mutex<String>,
}

impl SharedState {
    /// Creates a shared state with the given config and `idle` status.
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(SharedState {
            config: Mutex::new(config),
            status: Mutex::new(SyncStatus::Idle),
            syncing: Arc::new(AtomicBool::new(false)),
            quit_requested: AtomicBool::new(false),
            progress: Mutex::new((0, 0)),
            speed: Mutex::new(String::new()),
        })
    }

    /// Returns the current synchronization status.
    pub fn current_status(&self) -> SyncStatus {
        *self.status.lock().unwrap()
    }

    /// `true` if synchronization is paused according to the config.
    pub fn is_paused(&self) -> bool {
        self.config.lock().unwrap().paused
    }

    /// Requests daemon termination (sets `quit_requested` to `true`).
    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }
}

/// Main rich interface of the daemon.
pub struct Daemon {
    state: Arc<SharedState>,
}

// zbus generates the interface methods' documentation from each method's `///`
// comments, so `missing_docs` would raise false positives over the code
// expanded by the `#[interface]` macro.
#[interface(name = "com.gdrive.Daemon1")]
impl Daemon {
    /// Starts a manual synchronization. Returns a result message.
    async fn sync_now(
        &mut self,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> String {
        run_sync(&self.state, &emitter).await
    }

    /// Pauses synchronization (persists `paused=true`).
    async fn pause(
        &mut self,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) {
        set_paused(&self.state, true, &emitter).await;
    }

    /// Resumes synchronization (persists `paused=false`).
    async fn resume(
        &mut self,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) {
        set_paused(&self.state, false, &emitter).await;
    }

    /// Returns the current status as `a{ss}`.
    async fn get_status(&mut self) -> HashMap<String, String> {
        let cfg = self.state.config.lock().unwrap();
        let status = self.state.current_status();
        let mut m = HashMap::new();
        m.insert("status".into(), status.as_str().to_string());
        m.insert("paused".into(), cfg.paused.to_string());
        m.insert("version".into(), APP_VERSION.to_string());
        m.insert("last_sync".into(), cfg.last_sync.clone());
        m.insert("last_sync_ok".into(), cfg.last_sync_ok.to_string());
        m
    }

    /// Updates the configuration from an `a{ss}` (e.g. `sync_dir`, language).
    /// Returns `true` if there were persisted changes.
    async fn update_config(&mut self, changes: HashMap<String, String>) -> bool {
        let mut cfg = self.state.config.lock().unwrap();
        let mut changed = false;
        for (k, v) in changes {
            if apply_config_field(&mut cfg, &k, &v) {
                changed = true;
            }
        }
        if changed {
            let _ = save_config(&cfg);
        }
        changed
    }

    /// Requests the daemon to quit (the binary observes `quit_requested`).
    async fn quit(&mut self) {
        self.state.request_quit();
        log_write(
            &self.state.config.lock().unwrap(),
            Level::Info,
            "Daemon",
            "quit requested",
            false,
        );
    }

    /// Signal emitted when the synchronization status changes.
    #[zbus(signal)]
    async fn status_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        status: &str,
        paused: bool,
    ) -> zbus::Result<()>;

    /// Signal emitted for each rclone progress line: `(text, transferred, total)`.
    #[zbus(signal)]
    async fn progress(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        text: &str,
        transferred: u64,
        total: u64,
    ) -> zbus::Result<()>;

    /// Signal emitted when the transfer speed changes.
    #[zbus(signal)]
    async fn speed_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        speed: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when a sync finishes: `(ok, message)`.
    #[zbus(signal)]
    async fn sync_finished(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        ok: bool,
        message: &str,
    ) -> zbus::Result<()>;

    /// Returns the current progress `(transferred, total, speed)` as `(ttt)`.
    async fn get_progress(&mut self) -> (u64, u64, String) {
        let p = *self.state.progress.lock().unwrap();
        let s = self.state.speed.lock().unwrap().clone();
        (p.0, p.1, s)
    }
}

/// Implements `org.gtk.Actions` for compatibility with `gapplication action`.
/// Exposed at the same path `/com/gdrive/Client`.
pub struct GtkActions {
    state: Arc<SharedState>,
}

#[interface(name = "org.gtk.Actions")]
impl GtkActions {
    /// `List`: action names.
    async fn list(&self) -> Vec<String> {
        action_names()
    }

    /// `Activate`: invoked by GTK's ActionMap.
    async fn activate(
        &mut self,
        action_name: &str,
        _parameter: Vec<OwnedValue>,
        _platform_data: HashMap<String, OwnedValue>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) {
        dispatch_action(&self.state, action_name, &emitter).await;
    }
}

/// Implements `org.freedesktop.Application`, which is what
/// `gapplication action <app-id> <name>` actually invokes (via `ActivateAction`).
pub struct FdoApplication {
    state: Arc<SharedState>,
}

#[interface(name = "org.freedesktop.Application")]
impl FdoApplication {
    /// `Activate`: opens/activates the app (no-op: the daemon is already active).
    async fn activate(&mut self) {}

    /// `Open`: no-op (we don't handle URIs/file-schemes for now).
    async fn open(&mut self, _uris: Vec<String>, _platform_data: HashMap<String, OwnedValue>) {}

    /// `ActivateAction`: what triggers `gapplication action com.gdrive.Client <name>`.
    async fn activate_action(
        &mut self,
        action_name: &str,
        _parameter: Vec<OwnedValue>,
        _platform_data: HashMap<String, OwnedValue>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) {
        dispatch_action(&self.state, action_name, &emitter).await;
    }
}

/// Exposed action names (shared between `org.gtk.Actions.List` and docs).
fn action_names() -> Vec<String> {
    vec!["sync-now".into(), "pause".into(), "quit".into()]
}

/// Dispatches an action by name, shared between `org.gtk.Actions` and
/// `org.freedesktop.Application`.
async fn dispatch_action(
    state: &Arc<SharedState>,
    action_name: &str,
    emitter: &zbus::object_server::SignalEmitter<'_>,
) {
    match action_name {
        "sync-now" => {
            run_sync(state, emitter).await;
        }
        "pause" => set_paused(state, true, emitter).await,
        "quit" => {
            state.request_quit();
        }
        _ => {}
    }
}

/// Applies a configuration field from a stringified `(key, value)`.
fn apply_config_field(cfg: &mut Config, key: &str, value: &str) -> bool {
    match key {
        "sync_dir" => {
            cfg.sync_dir = value.to_string();
            true
        }
        "sync_interval" => {
            cfg.sync_interval = value.to_string();
            true
        }
        "watch_changes" => {
            if let Ok(b) = value.parse::<bool>() {
                let c = cfg.watch_changes != b;
                cfg.watch_changes = b;
                c
            } else {
                false
            }
        }
        "bandwidth_limit" => {
            cfg.bandwidth_limit = value.to_string();
            true
        }
        "download_limit" => {
            cfg.download_limit = value.to_string();
            true
        }
        "upload_limit" => {
            cfg.upload_limit = value.to_string();
            true
        }
        "sync_mode" => {
            cfg.sync_mode = value.to_string();
            true
        }
        "proxy_url" => {
            cfg.proxy_url = value.to_string();
            true
        }
        "log_level" => {
            cfg.log_level = value.to_string();
            true
        }
        "conflict_action" => {
            cfg.conflict_action = value.to_string();
            true
        }
        "language" => {
            cfg.language = value.to_string();
            true
        }
        "paused" => {
            if let Ok(b) = value.parse::<bool>() {
                let c = cfg.paused != b;
                cfg.paused = b;
                c
            } else {
                false
            }
        }
        "skip_hidden" => {
            if let Ok(b) = value.parse::<bool>() {
                let c = cfg.skip_hidden != b;
                cfg.skip_hidden = b;
                c
            } else {
                false
            }
        }
        "store_metadata" => {
            if let Ok(b) = value.parse::<bool>() {
                let c = cfg.store_metadata != b;
                cfg.store_metadata = b;
                c
            } else {
                false
            }
        }
        "num_threads" => {
            if let Ok(n) = value.parse::<u32>() {
                let c = cfg.num_threads != n;
                cfg.num_threads = n;
                c
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Runs the sync on a blocking thread in a **decoupled** way: returns
/// immediately (so as not to block the D-Bus call) and emits `StatusChanged`
/// when it ends. Returns an acceptance message ("started"/"already_syncing").
async fn run_sync(
    state: &Arc<SharedState>,
    emitter: &zbus::object_server::SignalEmitter<'_>,
) -> String {
    if state.syncing.swap(true, Ordering::SeqCst) {
        return "already_syncing".to_string();
    }

    set_status(state, SyncStatus::Syncing, emitter).await;

    let engine = { SyncEngine::new(state.config.lock().unwrap().clone()) };
    let state_clone = state.clone();

    // Capture the session connection to emit signals from the async task.
    let conn_for_signals = zbus::Connection::session().await.ok();

    // Decouple: launch the sync in background and respond now.
    tokio::task::spawn(async move {
        // Reset progress/speed at startup.
        {
            *state_clone.progress.lock().unwrap() = (0, 0);
            state_clone.speed.lock().unwrap().clear();
        }

        // Forwarding task: watches the shared state and emits D-Bus signals
        // while the sync is in progress (blocking thread + separate reactor).
        let state_poll = state_clone.clone();
        let conn_progress = conn_for_signals.clone();
        tokio::task::spawn(async move {
            let mut last_txt = String::new();
            let mut last_speed = String::new();
            loop {
                if !state_poll.syncing.load(Ordering::SeqCst) {
                    break;
                }
                let (tr, to) = *state_poll.progress.lock().unwrap();
                let speed = state_poll.speed.lock().unwrap().clone();
                if let Some(conn) = &conn_progress {
                    let path = daemon_object_path();
                    if let Ok(emitter) = zbus::object_server::SignalEmitter::new(conn, path) {
                        if tr > 0 || to > 0 {
                            let txt = format!("{tr} / {to}");
                            if txt != last_txt {
                                let _ = emitter.progress(&txt, tr, to).await;
                                last_txt = txt;
                            }
                        }
                        let spd: &str = if speed.is_empty() {
                            "—"
                        } else {
                            speed.as_str()
                        };
                        if spd != last_speed {
                            let _ = emitter.speed_changed(spd).await;
                            last_speed = spd.to_string();
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        });

        let state_hook = state_clone.clone();
        let result = tokio::task::spawn_blocking(move || {
            engine.run_with_callback(|line, transferred, total, speed| {
                let mut p = state_hook.progress.lock().unwrap();
                *p = (transferred, total);
                let mut s = state_hook.speed.lock().unwrap();
                if let Some(sp) = speed {
                    *s = sp.to_string();
                }
                let _ = line;
            })
        })
        .await;

        let mut ok = false;
        let message = match result {
            Ok(Ok(o)) => {
                ok = o.ok;
                {
                    let mut cfg = state_clone.config.lock().unwrap();
                    cfg.last_sync_ok = o.ok;
                    let _ = save_config(&cfg);
                }
                o.message
            }
            Ok(Err(e)) => {
                set_status_nosignal(&state_clone, SyncStatus::Error);
                log_write(
                    &state_clone.config.lock().unwrap(),
                    Level::Error,
                    "Daemon",
                    &e.to_string(),
                    true,
                );
                e.to_string()
            }
            Err(e) => {
                set_status_nosignal(&state_clone, SyncStatus::Error);
                format!("join error: {e}")
            }
        };

        state_clone.syncing.store(false, Ordering::SeqCst);

        let paused = state_clone.is_paused();
        let final_status = if paused {
            SyncStatus::Paused
        } else {
            SyncStatus::Idle
        };

        // Emit the end-of-sync signals.
        if let Some(conn) = &conn_for_signals {
            let path = daemon_object_path();
            if let Ok(emitter) = zbus::object_server::SignalEmitter::new(conn, path) {
                let _ = emitter.status_changed(final_status.as_str(), paused).await;
                let _ = emitter.sync_finished(ok, &message).await;
                let _ = emitter.speed_changed("—").await;
            }
        }

        set_status_nosignal(&state_clone, final_status);
        log_write(
            &state_clone.config.lock().unwrap(),
            Level::Info,
            "Daemon",
            &format!("sync finished: {message}"),
            true,
        );
    });

    "started".to_string()
}

/// Updates the status **without** emitting a signal (for use inside detached tasks).
fn set_status_nosignal(state: &Arc<SharedState>, status: SyncStatus) {
    let mut s = state.status.lock().unwrap();
    *s = status;
}

/// Updates the status + emits `StatusChanged`.
async fn set_status(
    state: &Arc<SharedState>,
    status: SyncStatus,
    emitter: &zbus::object_server::SignalEmitter<'_>,
) {
    set_status_nosignal(state, status);
    let paused = state.is_paused();
    let _ = emitter.status_changed(status.as_str(), paused).await;
}

async fn set_paused(
    state: &Arc<SharedState>,
    paused: bool,
    emitter: &zbus::object_server::SignalEmitter<'_>,
) {
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.paused = paused;
        let _ = save_config(&cfg);
    }
    log_write(
        &state.config.lock().unwrap(),
        Level::Info,
        "Daemon",
        if paused { "paused" } else { "resumed" },
        true,
    );
    let new_status = if paused {
        SyncStatus::Paused
    } else {
        SyncStatus::Idle
    };
    set_status(state, new_status, emitter).await;
}

/// Builds the connection and serves the interfaces. Returns the connection (keeping
/// it alive keeps the service active).
pub async fn build_server(state: Arc<SharedState>) -> zbus::Result<zbus::Connection> {
    let daemon = Daemon {
        state: state.clone(),
    };
    let actions = GtkActions {
        state: state.clone(),
    };
    let fdo_app = FdoApplication { state };

    let conn = zbus::connection::Builder::session()?
        .serve_at("/com/gdrive/Client", daemon)?
        .serve_at("/com/gdrive/Client", actions)?
        .serve_at("/com/gdrive/Client", fdo_app)?
        .build()
        .await?;

    // The name is requested **without queueing** (`DoNotQueue`), instead of via
    // `Builder::name()`: that variant queues the request, so a second
    // `gdrive-daemon` stayed alive without serving the bus but with its sync
    // scheduler and its watcher still active. Two engines running bisync over the
    // same pair is exactly what corrupted the listings.
    let reply = conn
        .request_name_with_flags(APP_ID, RequestNameFlags::DoNotQueue.into())
        .await?;
    if !matches!(reply, RequestNameReply::PrimaryOwner) {
        return Err(zbus::Error::NameTaken);
    }

    log_write(
        &load_config_or_default(),
        Level::Info,
        "Daemon",
        &format!("serving {APP_ID} at /com/gdrive/Client (v{APP_VERSION})"),
        false,
    );

    Ok(conn)
}

/// Service name and path (for tests/documentation).
pub fn service_details() -> (&'static str, &'static str) {
    (APP_ID, "/com/gdrive/Client")
}

/// The service's [`ObjectPath`], validated once. The literal is static and
/// known, so `expect` never fails in practice (avoids repeating
/// `try_from(...).unwrap()` at each signal-emission point).
fn daemon_object_path() -> zbus::zvariant::ObjectPath<'static> {
    use zbus::zvariant::ObjectPath;
    ObjectPath::try_from("/com/gdrive/Client").expect("path D-Bus estático válido")
}

/// Starts the periodic sync scheduler in the background.
///
/// Respects the explicit-control rule: **only** auto-sync if
/// `sync_interval` is set to a value other than `manual` and the sync is
/// not paused. If a sync is in progress, the tick is skipped
/// (never overlaps two syncs).
pub fn spawn_periodic_scheduler(state: Arc<SharedState>) {
    tokio::spawn(async move {
        loop {
            if state.quit_requested.load(Ordering::SeqCst) {
                break;
            }

            let (interval, paused) = {
                let cfg = state.config.lock().unwrap();
                (interval_seconds(&cfg.sync_interval), cfg.paused)
            };

            if let Some(secs) = interval {
                if !paused && !state.syncing.load(Ordering::SeqCst) {
                    // Trigger a sync without a method emitter: reuses run_sync
                    // with an emitter rebuilt from the session connection.
                    if let Ok(conn) = zbus::Connection::session().await {
                        let path = daemon_object_path();
                        if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, path) {
                            let _ = run_sync(&state, &emitter).await;
                        }
                    }
                }
                // Sleep the full interval between ticks.
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            } else {
                // `manual`: re-evaluate every 30s in case the config changes.
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    });
}

/// Starts real-time change detection in `sync_dir`.
///
/// Only acts if `config.watch_changes` is enabled (disabled by default,
/// respecting the user's explicit control). Creates a [`DirWatcher`] that
/// watches the folder and, after debouncing, triggers a sync reusing
/// [`run_sync`].
///
/// The task is **reactive**: it re-evaluates `watch_changes`/`sync_dir` every few
/// seconds, so turning the toggle on/off from the UI takes effect
/// without restarting the daemon. The watcher discards events while a sync is in
/// progress (avoids the feedback loop), using `state.syncing`.
pub fn spawn_change_watcher(state: Arc<SharedState>) {
    tokio::spawn(async move {
        let mut rx: Option<std::sync::mpsc::Receiver<()>> = None;
        let mut watched_dir: Option<std::path::PathBuf> = None;

        loop {
            if state.quit_requested.load(Ordering::SeqCst) {
                break;
            }

            let (watch, sync_dir) = {
                let cfg = state.config.lock().unwrap();
                (cfg.watch_changes, cfg.sync_dir.clone())
            };

            match (watch, &watched_dir) {
                // Start if enabled and we're not watching yet (or the folder changed).
                (true, None) => {
                    if !sync_dir.is_empty() {
                        let path = std::path::PathBuf::from(&sync_dir);
                        match start_watcher(&state, &path) {
                            Ok(r) => {
                                rx = Some(r);
                                watched_dir = Some(path);
                            }
                            Err(_) => {
                                // We will retry on the next cycle.
                            }
                        }
                    }
                }
                // Folder changed: restart the watcher on the new path.
                (true, Some(_)) => {
                    let current_is_new =
                        watched_dir.as_deref() != Some(std::path::Path::new(&sync_dir));
                    if current_is_new {
                        rx = None; // drop → stops the previous thread.
                        watched_dir = None;
                    }
                }
                // Disabled: stop.
                (false, Some(_)) => {
                    rx = None;
                    watched_dir = None;
                }
                (false, None) => {}
            }

            // Wait for an event or timeout to re-evaluate the config.
            if let Some(r) = rx.as_ref() {
                match r.recv_timeout(std::time::Duration::from_secs(3)) {
                    Ok(()) => {
                        // Change detected: trigger sync if appropriate.
                        if !state.is_paused() && !state.syncing.load(Ordering::SeqCst) {
                            trigger_sync_from_watcher(&state).await;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        // The watcher's internal thread died; restart.
                        rx = None;
                        watched_dir = None;
                    }
                }
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    });
}

/// Creates and starts a [`DirWatcher`] on `path`, recording the result.
fn start_watcher(
    state: &Arc<SharedState>,
    path: &std::path::Path,
) -> Result<std::sync::mpsc::Receiver<()>, ()> {
    if !path.exists() {
        log_write(
            &state.config.lock().unwrap(),
            Level::Error,
            "Watcher",
            &format!("sync_dir no existe: {}", path.display()),
            false,
        );
        return Err(());
    }
    let rx = DirWatcher::new(path.to_path_buf(), state.syncing.clone())
        .spawn()
        .map_err(|e| {
            log_write(
                &state.config.lock().unwrap(),
                Level::Error,
                "Watcher",
                &format!("no se pudo iniciar el watcher: {e}"),
                false,
            );
        })?;
    log_write(
        &state.config.lock().unwrap(),
        Level::Info,
        "Watcher",
        &format!("observando cambios en {}", path.display()),
        false,
    );
    Ok(rx)
}

/// Triggers a sync from the watcher, rebuilding the D-Bus emitter.
async fn trigger_sync_from_watcher(state: &Arc<SharedState>) {
    if let Ok(conn) = zbus::Connection::session().await {
        let path = daemon_object_path();
        if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, path) {
            let _ = run_sync(state, &emitter).await;
        }
    }
}
