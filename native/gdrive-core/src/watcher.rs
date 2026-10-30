//! Real-time change detection in `sync_dir`.
//!
//! Watches the file system (inotify on Linux via [`notify`]) recursively and,
//! after a period of calm (debounce), warns over an `mpsc` channel that there
//! are changes pending synchronization. The daemon consumes that channel and
//! triggers the sync.
//!
//! # Why the debounce
//!
//! Editors and file managers generate bursts of dozens of events for a single
//! action (saving a file produces `create` + `modify` + `close_write`). Without
//! a debounce we would trigger several redundant syncs. It waits for
//! [`DEBOUNCE`] of calm before signalling.
//!
//! # Feedback loops
//!
//! During a synchronization, rclone itself writes files into `sync_dir` (the
//! ones it brings from the remote). If the watcher reacted to those events, an
//! infinite loop would occur (sync → write → event → sync → …). To avoid it,
//! the watcher **discards events while the `syncing` flag is active**, shared
//! with the daemon through an [`Arc<AtomicBool>`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{RecursiveMode, Watcher};

use crate::error::{Error, Result};

/// Period of calm (with no new events) before signalling pending changes.
pub const DEBOUNCE: Duration = Duration::from_secs(5);

/// Milliseconds of the "never signalled" clock (no pending events).
const NO_EVENT: u64 = 0;

/// Watches a folder and emits "there are local changes" signals on a channel.
#[derive(Debug)]
pub struct DirWatcher {
    /// Observed path (absolute, pointing at `sync_dir`).
    path: PathBuf,
    /// Flag shared with the daemon: `true` while a sync is in progress.
    syncing: Arc<AtomicBool>,
    /// Timestamp (ms since epoch) of the last relevant event. `0` = nothing
    /// pending. Updated by the `notify` callback, consumed by the debouncer
    /// thread.
    last_event: Arc<AtomicU64>,
}

impl DirWatcher {
    /// Creates a watcher for `path`, ignoring events while `syncing` is
    /// `true` (avoids the feedback loop described above).
    pub fn new(path: PathBuf, syncing: Arc<AtomicBool>) -> Self {
        DirWatcher {
            path,
            syncing,
            last_event: Arc::new(AtomicU64::new(NO_EVENT)),
        }
    }

    /// Returns the observed path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Starts watching [`Self::path`] in the background and returns the receiver
    /// of signals. Each signal means "there are local changes after a period of
    /// calm"; the daemon decides whether to trigger the sync.
    ///
    /// The debouncer thread ends when the returned [`Receiver`] is dropped.
    pub fn spawn(self) -> Result<Receiver<()>> {
        let path = self.path.clone();
        // Synchronous check so the error is returned to the caller.
        if !path.exists() {
            return Err(Error::Rclone(format!(
                "watch dir no existe: {}",
                path.display()
            )));
        }

        let (tx, rx) = mpsc::channel::<()>();

        // Callback of the notification backend: stores the moment of the last
        // relevant event. `notify` invokes it on its own thread.
        let syncing = self.syncing.clone();
        let last_event = self.last_event.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else {
                return; // individual non-fatal errors (file already deleted, etc.)
            };
            if syncing.load(Ordering::SeqCst) {
                return; // anti-loop: do not react to our own sync.
            }
            if !is_relevant_event(&event) {
                return; // ignore irrelevant reads/metadata changes.
            }
            last_event.store(now_ms(), Ordering::SeqCst);
        })
        .map_err(|e| Error::Rclone(format!("crear watcher: {e}")))?;

        watcher
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|e| Error::Rclone(format!("watch {}: {e}", path.display())))?;

        // Debouncer thread: when [`DEBOUNCE`] goes by with no new events, it
        // signals. Keeps `watcher` alive (it is moved in here) and ends when
        // `rx` is dropped (detected by `tx.send` failing).
        let last_event_db = self.last_event; // move the last Arc; `self` is unused now.
        thread::spawn(move || {
            let _watcher = watcher; // keep the watcher alive while we observe.
            loop {
                thread::sleep(Duration::from_millis(100));
                let last = last_event_db.load(Ordering::SeqCst);
                if last != NO_EVENT && now_ms().saturating_sub(last) >= DEBOUNCE.as_millis() as u64
                {
                    // Consume the event to avoid re-signalling; if the owner
                    // already closed the channel (receiver dropped), we exit.
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

/// Current timestamp in milliseconds since the epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Decides whether a `notify` event counts as a "relevant content change".
///
/// Creations, data/name modifications and removals are kept; accesses (read
/// only) and metadata changes that do not alter the content are ignored.
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
