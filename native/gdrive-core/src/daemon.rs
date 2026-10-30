//! Daemon D-Bus de GDrive (Fase 2).
//!
//! Expone el motor (`SyncEngine`) a la UI GTK4 en Python mediante D-Bus. Sirve el
//! nombre de bus `com.gdrive.Client` (compatible con el tray icon actual, que
//! hace `gapplication action com.gdrive.Client <name>` vía `org.gtk.Actions`).
//!
//! Interfaz principal (para la UI): `com.gdrive.Daemon1`
//!   - `SyncNow() -> s`
//!   - `Pause()`, `Resume()`
//!   - `GetStatus() -> a{ss}`   (map: status, paused, version, ...)
//!   - `Quit()`
//!   - señal `StatusChanged(s status, b paused)`
//!
//! El sync corre en `tokio::task::spawn_blocking` (rclone es subproceso
//! bloqueante), de modo que el reactor nunca se bloquea.

//! El macro `#[interface]` de `zbus` genera el "glue" de las interfaces D-Bus
//! como items públicos sin documentar, disparando falsos positivos de
//! `missing_docs` sobre código que no es el nuestro. La documentación real de
//! cada método vive en sus comentarios `///` (que zbus usa para el XML de la
//! interfaz). Se desactiva el lint aquí, no en todo el crate.
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

/// Estado compartido reactor ↔ hilo de sync.
#[derive(Debug)]
pub struct SharedState {
    /// Configuración actual (protegida por mutex para acceso multi-hilo).
    pub config: Mutex<Config>,
    /// Estado de sincronización actual.
    pub status: Mutex<SyncStatus>,
    /// `true` mientras hay una sincronización en curso. `Arc` para compartirlo
    /// clonado con el watcher de cambios (que descarta eventos durante un sync).
    pub syncing: Arc<AtomicBool>,
    /// `true` cuando se ha solicitado la salida del daemon.
    pub quit_requested: AtomicBool,
    /// Progreso actual `(transferred, total)` del sync en curso.
    pub progress: Mutex<(u64, u64)>,
    /// Velocidad actual (formateada) del sync en curso.
    pub speed: Mutex<String>,
}

impl SharedState {
    /// Crea un estado compartido con la configuración dada y estado `idle`.
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

    /// Devuelve el estado de sincronización actual.
    pub fn current_status(&self) -> SyncStatus {
        *self.status.lock().unwrap()
    }

    /// `true` si la sincronización está pausada según la configuración.
    pub fn is_paused(&self) -> bool {
        self.config.lock().unwrap().paused
    }

    /// Solicita la terminación del daemon (pone `quit_requested` a `true`).
    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }
}

/// Interfaz rica principal del daemon.
pub struct Daemon {
    state: Arc<SharedState>,
}

// zbus genera la documentación de los métodos de la interfaz a partir de los
// comentarios `///` de cada método, por lo que `missing_docs` daría falsos
// positivos sobre el código expandido por el macro `#[interface]`.
#[interface(name = "com.gdrive.Daemon1")]
impl Daemon {
    /// Arranca una sincronización manual. Devuelve mensaje de resultado.
    async fn sync_now(
        &mut self,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> String {
        run_sync(&self.state, &emitter).await
    }

    /// Pausa la sincronización (persiste `paused=true`).
    async fn pause(
        &mut self,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) {
        set_paused(&self.state, true, &emitter).await;
    }

    /// Reanuda la sincronización (persiste `paused=false`).
    async fn resume(
        &mut self,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) {
        set_paused(&self.state, false, &emitter).await;
    }

    /// Devuelve el estado actual como `a{ss}`.
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

    /// Actualiza la configuración desde un `a{ss}` (p. ej. `sync_dir`, idioma).
    /// Devuelve `true` si hubo cambios persistidos.
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

    /// Solicita terminar el daemon (el binario observa `quit_requested`).
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

    /// Señal emitida cuando cambia el estado de sincronización.
    #[zbus(signal)]
    async fn status_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        status: &str,
        paused: bool,
    ) -> zbus::Result<()>;

    /// Señal emitida por cada línea de progreso de rclone: `(texto, transferido, total)`.
    #[zbus(signal)]
    async fn progress(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        text: &str,
        transferred: u64,
        total: u64,
    ) -> zbus::Result<()>;

    /// Señal emitida cuando cambia la velocidad de transferencia.
    #[zbus(signal)]
    async fn speed_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        speed: &str,
    ) -> zbus::Result<()>;

    /// Señal emitida al terminar un sync: `(ok, mensaje)`.
    #[zbus(signal)]
    async fn sync_finished(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        ok: bool,
        message: &str,
    ) -> zbus::Result<()>;

    /// Devuelve el progreso actual `(transferred, total, speed)` como `(ttt)`.
    async fn get_progress(&mut self) -> (u64, u64, String) {
        let p = *self.state.progress.lock().unwrap();
        let s = self.state.speed.lock().unwrap().clone();
        (p.0, p.1, s)
    }
}

/// Implementa `org.gtk.Actions` para compatibilidad con `gapplication action`.
/// Se expone en el mismo path `/com/gdrive/Client`.
pub struct GtkActions {
    state: Arc<SharedState>,
}

#[interface(name = "org.gtk.Actions")]
impl GtkActions {
    /// `List`: nombres de acciones.
    async fn list(&self) -> Vec<String> {
        action_names()
    }

    /// `Activate`: invocado por el ActionMap de GTK.
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

/// Implementa `org.freedesktop.Application`, que es lo que realmente invoca
/// `gapplication action <app-id> <name>` (vía `ActivateAction`).
pub struct FdoApplication {
    state: Arc<SharedState>,
}

#[interface(name = "org.freedesktop.Application")]
impl FdoApplication {
    /// `Activate`: abre/activa la app (no-op: el daemon ya está activo).
    async fn activate(&mut self) {}

    /// `Open`: no-op (no gestionamos URIs/file-schemes por ahora).
    async fn open(&mut self, _uris: Vec<String>, _platform_data: HashMap<String, OwnedValue>) {}

    /// `ActivateAction`: lo que dispara `gapplication action com.gdrive.Client <name>`.
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

/// Nombres de acciones expuestas (compartidos entre `org.gtk.Actions.List` y doc).
fn action_names() -> Vec<String> {
    vec!["sync-now".into(), "pause".into(), "quit".into()]
}

/// Despacha una acción por nombre, compartida entre `org.gtk.Actions` y
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

/// Aplica un campo de configuración a partir de `(key, value)` en string.
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

/// Ejecuta el sync en un hilo de bloqueo de forma **desacoplada**: devuelve
/// inmediatamente (para no bloquear la llamada D-Bus) y emite `StatusChanged`
/// al terminar. Devuelve un mensaje de aceptación ("started"/"already_syncing").
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

    // Capturar la conexión de sesión para emitir señales desde el task async.
    let conn_for_signals = zbus::Connection::session().await.ok();

    // Desacoplar: lanzar el sync en background y responder ya.
    tokio::task::spawn(async move {
        // Resetear progreso/velocidad al arrancar.
        {
            *state_clone.progress.lock().unwrap() = (0, 0);
            state_clone.speed.lock().unwrap().clear();
        }

        // Task de reenvío: observa el estado compartido y emite señales D-Bus
        // mientras el sync esté en curso (hilo bloqueante + reactor separados).
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

        // Emitir las señales de fin de sync.
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

/// Actualiza estado **sin** emitir señal (para usar dentro de tareas separadas).
fn set_status_nosignal(state: &Arc<SharedState>, status: SyncStatus) {
    let mut s = state.status.lock().unwrap();
    *s = status;
}

/// Actualiza estado + emite `StatusChanged`.
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

/// Construye la conexión y sirve las interfaces. Devuelve la conexión (mantenerla
/// viva mantiene el servicio activo).
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

    // El nombre se pide **sin cola** (`DoNotQueue`), en lugar de con
    // `Builder::name()`: esa variante encola la petición, así que un segundo
    // `gdrive-daemon` quedaba vivo sin atender el bus pero con su programador de
    // sync y su watcher activos. Dos motores corriendo bisync sobre el mismo par
    // es justo lo que corrompía los listados.
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

/// Nombre y path del servicio (para tests/documentación).
pub fn service_details() -> (&'static str, &'static str) {
    (APP_ID, "/com/gdrive/Client")
}

/// [`ObjectPath`] del servicio, validado una sola vez. El literal es estático y
/// conocido, de modo que `expect` nunca falla en la práctica (evita repetir
/// `try_from(...).unwrap()` en cada punto de emisión de señales).
fn daemon_object_path() -> zbus::zvariant::ObjectPath<'static> {
    use zbus::zvariant::ObjectPath;
    ObjectPath::try_from("/com/gdrive/Client").expect("path D-Bus estático válido")
}

/// Arranca el programador de sincronización periódica en segundo plano.
///
/// Respeta la regla de control explícito: **solo** sync automática si
/// `sync_interval` está configurado con un valor distinto de `manual` y el
/// sync no está pausado. Si hay una sincronización en curso, se salta el tick
/// (nunca solapa dos syncs).
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
                    // Disparar un sync sin emitter de método: reutiliza run_sync
                    // con un emitter reconstruido desde la conexión de sesión.
                    if let Ok(conn) = zbus::Connection::session().await {
                        let path = daemon_object_path();
                        if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, path) {
                            let _ = run_sync(&state, &emitter).await;
                        }
                    }
                }
                // Dormir el intervalo completo entre ticks.
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            } else {
                // `manual`: re-evaluar cada 30s por si cambia la config.
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    });
}

/// Arranca la detección de cambios en tiempo real en `sync_dir`.
///
/// Solo actúa si `config.watch_changes` está habilitado (desactivado por
/// defecto, respetando el control explícito del usuario). Crea un
/// [`DirWatcher`] que observa la carpeta y, tras el debounce, dispara un sync
/// reutilizando [`run_sync`].
///
/// El task es **reactivo**: re-evalúa `watch_changes`/`sync_dir` cada pocos
/// segundos, de modo que activar/desactivar el toggle desde la UI surte efecto
/// sin reiniciar el daemon. El watcher descarta eventos mientras hay un sync en
/// curso (evita el bucle de realimentación), usando `state.syncing`.
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
                // Arrancar si está activo y aún no observamos (o cambió la carpeta).
                (true, None) => {
                    if !sync_dir.is_empty() {
                        let path = std::path::PathBuf::from(&sync_dir);
                        match start_watcher(&state, &path) {
                            Ok(r) => {
                                rx = Some(r);
                                watched_dir = Some(path);
                            }
                            Err(_) => {
                                // Reintentaremos en el siguiente ciclo.
                            }
                        }
                    }
                }
                // Carpeta cambiada: reiniciar el watcher sobre la nueva ruta.
                (true, Some(_)) => {
                    let current_is_new =
                        watched_dir.as_deref() != Some(std::path::Path::new(&sync_dir));
                    if current_is_new {
                        rx = None; // soltar → detiene el hilo anterior.
                        watched_dir = None;
                    }
                }
                // Desactivado: detener.
                (false, Some(_)) => {
                    rx = None;
                    watched_dir = None;
                }
                (false, None) => {}
            }

            // Esperar evento o timeout para re-evaluar config.
            if let Some(r) = rx.as_ref() {
                match r.recv_timeout(std::time::Duration::from_secs(3)) {
                    Ok(()) => {
                        // Cambio detectado: disparar sync si procede.
                        if !state.is_paused() && !state.syncing.load(Ordering::SeqCst) {
                            trigger_sync_from_watcher(&state).await;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        // El hilo interno del watcher murió; reiniciar.
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

/// Crea y arranca un [`DirWatcher`] sobre `path`, registrando el resultado.
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

/// Dispara un sync desde el watcher, reconstruyendo el emitter de D-Bus.
async fn trigger_sync_from_watcher(state: &Arc<SharedState>) {
    if let Ok(conn) = zbus::Connection::session().await {
        let path = daemon_object_path();
        if let Ok(emitter) = zbus::object_server::SignalEmitter::new(&conn, path) {
            let _ = run_sync(state, &emitter).await;
        }
    }
}
