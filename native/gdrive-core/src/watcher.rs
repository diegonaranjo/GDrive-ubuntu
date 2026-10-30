//! Detección de cambios en tiempo real en `sync_dir`.
//!
//! Observa el sistema de archivos (inotify en Linux vía [`notify`]) de forma
//! recursiva y, tras un período de calma (debounce), avisa por un canal `mpsc`
//! de que hay cambios pendientes de sincronizar. El daemon consume ese canal y
//! dispara el sync.
//!
//! # Por qué el debounce
//!
//! Los editores y gestores de archivos generan ráfagas de decenas de eventos por
//! una sola acción (guardar un archivo produce `create` + `modify` +
//! `close_write`). Sin debounce, dispararíamos varios syncs redundantes. Se
//! espera [`DEBOUNCE`] de calma antes de señalar.
//!
//! # Bucles de realimentación
//!
//! Durante una sincronización, el propio rclone escribe archivos en `sync_dir`
//! (los que trae del remoto). Si el watcher reaccionara a esos eventos, se
//! produciría un bucle infinito (sync → escritura → evento → sync → …). Para
//! evitarlo, el watcher **descarta eventos mientras el flag `syncing` está
//! activo**, compartido con el daemon a través de un [`Arc<AtomicBool>`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{RecursiveMode, Watcher};

use crate::error::{Error, Result};

/// Tiempo de calma (sin nuevos eventos) antes de señalar cambios pendientes.
pub const DEBOUNCE: Duration = Duration::from_secs(5);

/// Milisegundos del reloj de "nunca señalizado" (sin eventos pendientes).
const NO_EVENT: u64 = 0;

/// Vigila una carpeta y emite señales de "hay cambios locales" por un canal.
#[derive(Debug)]
pub struct DirWatcher {
    /// Ruta observada (absoluta a `sync_dir`).
    path: PathBuf,
    /// Flag compartido con el daemon: `true` mientras hay un sync en curso.
    syncing: Arc<AtomicBool>,
    /// Marca de tiempo (ms desde epoch) del último evento relevante. `0` = nada
    /// pendiente. Actualizada por el callback de `notify`, consumida por el hilo
    /// debouncer.
    last_event: Arc<AtomicU64>,
}

impl DirWatcher {
    /// Crea un watcher para `path`, ignorando eventos mientras `syncing` sea
    /// `true` (evita el bucle de realimentación descrito arriba).
    pub fn new(path: PathBuf, syncing: Arc<AtomicBool>) -> Self {
        DirWatcher {
            path,
            syncing,
            last_event: Arc::new(AtomicU64::new(NO_EVENT)),
        }
    }

    /// Devuelve la ruta observada.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Lanza la observación de [`Self::path`] en segundo plano y devuelve el
    /// receptor de señales. Cada señal indica "hay cambios locales después de un
    /// período de calma"; el daemon decide disparar el sync.
    ///
    /// El hilo debouncer termina cuando el [`Receiver`] devuelto se suelta.
    pub fn spawn(self) -> Result<Receiver<()>> {
        let path = self.path.clone();
        // Comprobación síncrona para devolver el error al llamador.
        if !path.exists() {
            return Err(Error::Rclone(format!(
                "watch dir no existe: {}",
                path.display()
            )));
        }

        let (tx, rx) = mpsc::channel::<()>();

        // Callback del backend de notificación: marca el momento del último
        // evento relevante. `notify` lo invoca en su propio hilo.
        let syncing = self.syncing.clone();
        let last_event = self.last_event.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else {
                return; // errores individuales no fatales (archivo ya borrado, etc.)
            };
            if syncing.load(Ordering::SeqCst) {
                return; // anti-bucle: no reaccionar al propio sync.
            }
            if !is_relevant_event(&event) {
                return; // ignorar accesos/metadata irrelevantes.
            }
            last_event.store(now_ms(), Ordering::SeqCst);
        })
        .map_err(|e| Error::Rclone(format!("crear watcher: {e}")))?;

        watcher
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|e| Error::Rclone(format!("watch {}: {e}", path.display())))?;

        // Hilo debouncer: cuando pasa [`DEBOUNCE`] sin nuevos eventos, señaliza.
        // Mantiene `watcher` vivo (se mueve aquí) y termina al soltar `rx`
        // (detectado por el fallo de `tx.send`).
        let last_event_db = self.last_event; // mover el último Arc; `self` ya no se usa.
        thread::spawn(move || {
            let _watcher = watcher; // mantener el watcher vivo mientras observamos.
            loop {
                thread::sleep(Duration::from_millis(100));
                let last = last_event_db.load(Ordering::SeqCst);
                if last != NO_EVENT && now_ms().saturating_sub(last) >= DEBOUNCE.as_millis() as u64
                {
                    // Consumir el evento evitando re-señalizar; si el dueño ya
                    // cerró el canal (receptor soltado), terminamos.
                    last_event_db.store(NO_EVENT, Ordering::SeqCst);
                    if tx.send(()).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}

/// Marca de tiempo actual en milisegundos desde el epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Decide si un evento de `notify` cuenta como "cambio de contenido relevante".
///
/// Se quedan creaciones, modificaciones de datos/nombres y borrados; se ignoran
/// accesos (solo lectura) y cambios de metadata que no alteran el contenido.
pub fn is_relevant_event(event: &notify::Event) -> bool {
    use notify::event::ModifyKind;
    use notify::EventKind;
    matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Remove(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevant_event_detects_create_modify_remove() {
        use notify::event::{CreateKind, ModifyKind, RemoveKind};
        use notify::EventKind;

        let create = notify::Event::new(EventKind::Create(CreateKind::File));
        assert!(is_relevant_event(&create));

        let modify = notify::Event::new(EventKind::Modify(ModifyKind::Data(
            notify::event::DataChange::Any,
        )));
        assert!(is_relevant_event(&modify));

        let rename = notify::Event::new(EventKind::Modify(ModifyKind::Name(
            notify::event::RenameMode::Any,
        )));
        assert!(is_relevant_event(&rename));

        let remove = notify::Event::new(EventKind::Remove(RemoveKind::File));
        assert!(is_relevant_event(&remove));
    }

    #[test]
    fn irrelevant_access_event_is_ignored() {
        use notify::event::AccessKind;
        use notify::EventKind;

        let access = notify::Event::new(EventKind::Access(AccessKind::Read));
        assert!(!is_relevant_event(&access));
    }

    #[test]
    fn debounce_is_positive_duration() {
        assert!(DEBOUNCE.as_millis() > 0);
    }
}
