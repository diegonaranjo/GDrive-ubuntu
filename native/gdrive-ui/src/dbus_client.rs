//! D-Bus client for the native core daemon (`com.gdrive.Daemon1`).
//!
//! Rust port of `gdrive/daemon_client.py`, using GIO (`gio::DBusConnection`)
//! instead of an async runtime. This integrates with the GTK main loop: calls
//! are synchronous, and the daemon's signals (`StatusChanged`, `Progress`,
//! `SpeedChanged`, `SyncFinished`) are delivered via `subscribe_to_signal` on
//! the same loop, exactly like the Python `DaemonClient`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gio::DBusConnection;
use glib::prelude::*;

/// D-Bus well-known name owned by `gdrive-core`'s daemon.
pub const DAEMON_ID: &str = "com.gdrive.Client";
/// D-Bus object path served by the daemon.
pub const DAEMON_PATH: &str = "/com/gdrive/Client";
/// D-Bus interface for the rich API (`SyncNow`, `GetStatus`, signals, …).
pub const DAEMON_INTERFACE: &str = "com.gdrive.Daemon1";

/// Typed error for [`DaemonClient`] operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A D-Bus method call returned an error.
    #[error("D-Bus call failed: {0}")]
    Call(String),
    /// The reply variant could not be deserialized into the expected shape.
    #[error("unexpected reply type: {0}")]
    Variant(String),
}

impl From<glib::Error> for Error {
    fn from(e: glib::Error) -> Self {
        Error::Call(e.to_string())
    }
}

/// Result alias for the client.
pub type Result<T> = std::result::Result<T, Error>;

/// Snapshot of the daemon's sync status returned by `GetStatus`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Current state: `idle`, `syncing`, `paused`, or `error`.
    pub state: String,
    /// Whether sync is paused.
    pub paused: bool,
    /// Daemon version string.
    pub version: String,
    /// Human-readable last sync timestamp (empty = never).
    pub last_sync: String,
}

/// A single live signal subscription (kept to prevent it being dropped).
type Subscription = gio::SignalSubscription;

/// Client for the daemon's `com.gdrive.Daemon1` interface.
#[derive(Clone)]
pub struct DaemonClient {
    conn: DBusConnection,
    /// Live subscriptions, retained so they are not cancelled on drop.
    subs: Rc<RefCell<Vec<Subscription>>>,
}

impl DaemonClient {
    /// Connects to the session bus. Returns `None` if the bus is unreachable.
    pub fn connect() -> Option<Self> {
        let conn = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
        Some(DaemonClient {
            conn,
            subs: Rc::new(RefCell::new(Vec::new())),
        })
    }

    /// Returns `true` if the daemon owns its well-known name.
    #[allow(dead_code)] // used by the Settings page in a later phase
    pub fn is_available(&self) -> bool {
        self.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&(DAEMON_ID,).to_variant()),
        )
        .ok()
        .and_then(|v| v.get::<bool>())
        .unwrap_or(false)
    }

    /// Fetches the current status via `GetStatus` (`a{ss}`).
    pub fn get_status(&self) -> Result<Status> {
        let reply = self.call_daemon("GetStatus", None::<&glib::Variant>)?;
        let map: HashMap<String, String> = reply
            .get::<HashMap<String, String>>()
            .ok_or_else(|| Error::Variant("expected a{ss}".into()))?;
        Ok(Status::from_map(map))
    }

    /// Triggers a manual sync via `SyncNow`. Returns the daemon's result message.
    pub fn sync_now(&self) -> Result<String> {
        let reply = self.call_daemon("SyncNow", None::<&glib::Variant>)?;
        reply
            .get::<String>()
            .ok_or_else(|| Error::Variant("expected s".into()))
    }

    /// Pauses sync via `Pause`.
    pub fn pause(&self) -> Result<()> {
        let _ = self.call_daemon("Pause", None::<&glib::Variant>)?;
        Ok(())
    }

    /// Resumes sync via `Resume`.
    pub fn resume(&self) -> Result<()> {
        let _ = self.call_daemon("Resume", None::<&glib::Variant>)?;
        Ok(())
    }

    /// Pushes configuration keys via `UpdateConfig`. Returns `true` if changed.
    #[allow(dead_code)] // used by the Sync/Settings pages in a later phase
    pub fn update_config(&self, changes: HashMap<String, String>) -> Result<bool> {
        let reply = self.call_daemon("UpdateConfig", Some(&changes.to_variant()))?;
        reply
            .get::<bool>()
            .ok_or_else(|| Error::Variant("expected b".into()))
    }

    /// Subscribes to a daemon signal, decoding each emission into a tuple via
    /// `decode`. The subscription is retained for the client's lifetime.
    pub fn subscribe<F, T>(&self, signal: &str, decode: fn(&glib::Variant) -> Option<T>, handler: F)
    where
        F: Fn(T) + 'static,
        T: 'static,
    {
        let handle = self.conn.subscribe_to_signal(
            Some(DAEMON_ID),
            Some(DAEMON_INTERFACE),
            Some(signal),
            Some(DAEMON_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |sig| {
                if let Some(decoded) = decode(sig.parameters) {
                    handler(decoded);
                }
            },
        );
        self.subs.borrow_mut().push(handle);
    }

    /// Subscribes to `StatusChanged (s status, b paused)`.
    pub fn on_status_changed<F>(&self, handler: F)
    where
        F: Fn(String, bool) + 'static,
    {
        self.subscribe("StatusChanged", decode_status_changed, move |(s, p)| {
            handler(s, p)
        });
    }

    /// Subscribes to `Progress (s text, t transferred, t total)`.
    pub fn on_progress<F>(&self, handler: F)
    where
        F: Fn(String, u64, u64) + 'static,
    {
        self.subscribe("Progress", decode_progress, move |(t, tr, to)| {
            handler(t, tr, to)
        });
    }

    /// Subscribes to `SpeedChanged (s speed)`.
    pub fn on_speed_changed<F>(&self, handler: F)
    where
        F: Fn(String) + 'static,
    {
        self.subscribe("SpeedChanged", decode_speed, handler);
    }

    /// Subscribes to `SyncFinished (b ok, s message)`.
    pub fn on_sync_finished<F>(&self, handler: F)
    where
        F: Fn(bool, String) + 'static,
    {
        self.subscribe("SyncFinished", decode_sync_finished, move |(ok, msg)| {
            handler(ok, msg)
        });
    }

    /// Calls a method on the daemon interface with no meaningful body.
    fn call_daemon(&self, method: &str, params: Option<&glib::Variant>) -> Result<glib::Variant> {
        self.call(DAEMON_ID, DAEMON_PATH, DAEMON_INTERFACE, method, params)
    }

    /// Low-level `call_sync` wrapper.
    fn call(
        &self,
        bus_name: &str,
        object_path: &str,
        interface: &str,
        method: &str,
        params: Option<&glib::Variant>,
    ) -> Result<glib::Variant> {
        let reply = self
            .conn
            .call_sync(
                Some(bus_name),
                object_path,
                interface,
                method,
                params,
                None,
                gio::DBusCallFlags::NONE,
                2000,
                None::<&gio::Cancellable>,
            )
            .map_err(Error::from)?;
        Ok(reply)
    }
}

impl Status {
    /// Parses a `GetStatus` reply (`a{ss}`) into a [`Status`].
    fn from_map(map: HashMap<String, String>) -> Self {
        Status {
            state: map.get("status").cloned().unwrap_or_else(|| "idle".into()),
            paused: map.get("paused").is_some_and(|v| v == "true"),
            version: map
                .get("version")
                .cloned()
                .unwrap_or_else(|| "unknown".into()),
            last_sync: map.get("last_sync").cloned().unwrap_or_default(),
        }
    }
}

/// Decodes `StatusChanged (s, b)`.
fn decode_status_changed(v: &glib::Variant) -> Option<(String, bool)> {
    let (s, b): (String, bool) = v.get()?;
    Some((s, b))
}

/// Decodes `Progress (s, t, t)`.
fn decode_progress(v: &glib::Variant) -> Option<(String, u64, u64)> {
    let (t, tr, to): (String, u64, u64) = v.get()?;
    Some((t, tr, to))
}

/// Decodes `SpeedChanged (s)`.
fn decode_speed(v: &glib::Variant) -> Option<String> {
    v.get::<String>()
}

/// Decodes `SyncFinished (b, s)`.
fn decode_sync_finished(v: &glib::Variant) -> Option<(bool, String)> {
    let (ok, msg): (bool, String) = v.get()?;
    Some((ok, msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_parses_daemon_map() {
        let mut m = HashMap::new();
        m.insert("status".to_string(), "syncing".to_string());
        m.insert("paused".to_string(), "false".to_string());
        m.insert("version".to_string(), "1.0.0".to_string());
        m.insert("last_sync".to_string(), "today".to_string());
        let s = Status::from_map(m);
        assert_eq!(s.state, "syncing");
        assert!(!s.paused);
        assert_eq!(s.version, "1.0.0");
        assert_eq!(s.last_sync, "today");
    }

    #[test]
    fn status_defaults_when_keys_missing() {
        let s = Status::from_map(HashMap::new());
        assert_eq!(s.state, "idle");
        assert!(!s.paused);
        assert_eq!(s.version, "unknown");
        assert!(s.last_sync.is_empty());
    }

    #[test]
    fn status_paused_flag() {
        let mut m = HashMap::new();
        m.insert("paused".to_string(), "true".to_string());
        assert!(Status::from_map(m).paused);
    }

    #[test]
    fn decode_status_changed_roundtrip() {
        let v = ("syncing".to_string(), true).to_variant();
        assert_eq!(decode_status_changed(&v), Some(("syncing".into(), true)));
    }

    #[test]
    fn decode_progress_roundtrip() {
        let v = ("10 / 20".to_string(), 10u64, 20u64).to_variant();
        assert_eq!(decode_progress(&v), Some(("10 / 20".into(), 10, 20)));
    }
}
