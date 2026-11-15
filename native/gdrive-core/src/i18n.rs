//! ES/EN internationalization with per-file override, mirror of `gdrive/i18n.py`.
//!
//! Keys such as `status.idle`, `sync.finished_ok`, etc. with `format!`-style
//! placeholders (`{used}`, `{total}`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

static STATE: OnceLock<RwLock<I18n>> = OnceLock::new();

struct I18n {
    lang: String,
    translations: HashMap<&'static str, &'static str>,
}

/// Initializes (or reconfigures) the active language. Safe to call several times.
pub fn set_language(lang: &str) {
    let st = STATE.get_or_init(|| {
        RwLock::new(I18n {
            lang: "es".into(),
            translations: HashMap::new(),
        })
    });
    let mut guard = st.write().expect("i18n lock");
    guard.lang = lang.to_string();
    guard.translations = builtin(lang);
}

/// Returns the active language code (e.g. `"es"`).
pub fn get_language() -> String {
    let st = STATE.get_or_init(|| {
        RwLock::new(I18n {
            lang: "es".into(),
            translations: HashMap::new(),
        })
    });
    st.read().expect("i18n lock").lang.clone()
}

/// Translates a key. If missing, returns the key itself (like the Python).
pub fn t(key: &str) -> String {
    let st = STATE.get_or_init(|| {
        RwLock::new(I18n {
            lang: "es".into(),
            translations: builtin("es"),
        })
    });
    let guard = st.read().expect("i18n lock");
    // 1) user per-file override (if it exists), 2) builtin, 3) the key.
    override_file(&guard.lang, key)
        .or_else(|| guard.translations.get(key).map(|s| s.to_string()))
        .unwrap_or_else(|| key.to_string())
}

/// Translates with positional arguments in Python `range` style: `{0}`, `{1}`…
/// Only substitutes simple placeholders (no format specifiers).
pub fn t_args(key: &str, args: &[&str]) -> String {
    let mut s = t(key);
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}

/// List of available languages as `(code, human-readable name)`.
pub fn available_languages() -> Vec<(&'static str, &'static str)> {
    vec![("es", "Español"), ("en", "English")]
}

/// Sorted keys bundled for `lang`.
///
/// Exposed for the catalog parity test: `es` and `en` must always expose the
/// same key set, and a key added to one language only is a silent leak of the
/// key name to the user.
pub fn catalog_keys(lang: &str) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = builtin(lang).keys().copied().collect();
    keys.sort_unstable();
    keys
}

/// User override: `~/.config/gdrive/translations_{lang}.json`.
fn override_file(lang: &str, key: &str) -> Option<String> {
    let path: PathBuf = PathBuf::from(std::env::var_os("HOME")?)
        .join(".config/gdrive")
        .join(format!("translations_{lang}.json"));
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(&path).ok()?;
    let map: HashMap<String, String> = serde_json::from_str(&raw).ok()?;
    map.get(key).cloned()
}

/// Bundled translations. The subset relevant to the core (sync/logging/
/// states). The full UI lives in the GTK layer and keeps its own catalog.
fn builtin(lang: &str) -> HashMap<&'static str, &'static str> {
    match lang {
        "es" => HashMap::from([
            ("status.idle", "Listo"),
            ("status.syncing", "Sincronizando…"),
            ("status.paused", "Pausado"),
            ("status.error", "Error"),
            ("sync_manager.starting", "Iniciando sincronización"),
            ("sync_manager.completed", "Sincronización completada"),
            ("sync_manager.failed", "Error en sincronización"),
            ("sync_manager.paused", "Pausado"),
            ("sync_manager.resumed", "Reanudado"),
            ("sync.finished_ok", "Sincronización completada"),
            ("sync.finished_error", "Error de sincronización"),
            ("account.storage_used", "{used} de {total} usados"),
            ("account.no_limit", "Sin límite"),
            ("account.no_storage_info", "No se pudo obtener información"),
            ("account.not_signed_in", "Not signed in"),
            ("account.google_drive_account", "Google Drive account"),
            // ── Runtime errors (resolved by `Error::message`) ──
            ("error.serde", "config JSON inválido: {0}"),
            (
                "error.sync_timeout",
                "rclone sin actividad: se terminó el sync por estancamiento",
            ),
            ("error.process_exit", "subproceso salió con código {0}"),
            (
                "error.save_config",
                "no se pudo persistir la configuración en {0}: {1}",
            ),
            (
                "error.invalid_config",
                "valor de configuración inválido para '{0}': {1}",
            ),
            // ── Log messages (component tags stay untranslated) ──
            ("log.sync.no_stderr", "sin stderr disponible"),
            ("log.sync.no_transfers", "sin transferencias"),
            (
                "log.sync.orphan_lock_removed",
                "Lock huérfano eliminado: {0}",
            ),
            (
                "log.sync.lock_unreadable",
                "No se pudo leer el lock de bisync: {0}",
            ),
            ("log.daemon.sync_dir_missing", "sync_dir no existe: {0}"),
            (
                "log.daemon.watcher_start_failed",
                "no se pudo iniciar el watcher: {0}",
            ),
            ("log.daemon.watching", "observando cambios en {0}"),
            ("log.watcher.dir_missing", "watch dir no existe: {0}"),
            ("log.watcher.create_failed", "crear watcher: {0}"),
        ]),
        "en" => HashMap::from([
            ("status.idle", "Ready"),
            ("status.syncing", "Syncing…"),
            ("status.paused", "Paused"),
            ("status.error", "Error"),
            ("sync_manager.starting", "Starting sync"),
            ("sync_manager.completed", "Sync completed"),
            ("sync_manager.failed", "Sync error"),
            ("sync_manager.paused", "Paused"),
            ("sync_manager.resumed", "Resumed"),
            ("sync.finished_ok", "Sync completed"),
            ("sync.finished_error", "Sync error"),
            ("account.storage_used", "{used} of {total} used"),
            ("account.no_limit", "No limit"),
            ("account.no_storage_info", "Could not retrieve information"),
            ("account.not_signed_in", "Not signed in"),
            ("account.google_drive_account", "Google Drive account"),
            // ── Runtime errors (resolved by `Error::message`) ──
            ("error.serde", "invalid config JSON: {0}"),
            (
                "error.sync_timeout",
                "rclone went idle: sync aborted after stalling",
            ),
            ("error.process_exit", "subprocess exited with code {0}"),
            (
                "error.save_config",
                "could not persist the configuration to {0}: {1}",
            ),
            (
                "error.invalid_config",
                "invalid configuration value for '{0}': {1}",
            ),
            // ── Log messages (component tags stay untranslated) ──
            ("log.sync.no_stderr", "no stderr available"),
            ("log.sync.no_transfers", "no transfers"),
            ("log.sync.orphan_lock_removed", "Orphaned lock removed: {0}"),
            (
                "log.sync.lock_unreadable",
                "Could not read the bisync lock: {0}",
            ),
            (
                "log.daemon.sync_dir_missing",
                "sync_dir does not exist: {0}",
            ),
            (
                "log.daemon.watcher_start_failed",
                "could not start the watcher: {0}",
            ),
            ("log.daemon.watching", "watching for changes in {0}"),
            ("log.watcher.dir_missing", "watch dir does not exist: {0}"),
            ("log.watcher.create_failed", "create watcher: {0}"),
        ]),
        _ => builtin("es"),
    }
}
