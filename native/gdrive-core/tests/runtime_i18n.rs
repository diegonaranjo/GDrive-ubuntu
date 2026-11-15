//! Regression tests for the runtime strings of issue #1 (S1/S2/S4/S5/S6).
//!
//! The `es` catalog has to reproduce, byte for byte, the text these call sites
//! emitted before the change; otherwise "localizing" the strings silently
//! rewrote the default-language output.

use std::sync::Mutex;

use gdrive_core::error::Error;
use gdrive_core::i18n::{catalog_keys, set_language, t, t_args};

/// `set_language` is process-global: serialize the tests that switch it.
static I18N_LOCK: Mutex<()> = Mutex::new(());

fn guard() -> std::sync::MutexGuard<'static, ()> {
    I18N_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn es_catalog_reproduces_the_pre_change_output() {
    let _g = guard();
    set_language("es");
    // S1.4 — errors, exactly as the old `#[error(...)]` bodies rendered.
    let serde_err = serde_json::from_str::<u8>("x").unwrap_err();
    // Built from the error itself: the detail belongs to serde_json and its
    // wording is not ours to pin.
    let expected_serde = format!("config JSON inválido: {serde_err}");
    assert_eq!(Error::Serde(serde_err).message(), expected_serde);
    assert_eq!(
        Error::SyncTimeout.message(),
        "rclone sin actividad: se terminó el sync por estancamiento"
    );
    assert_eq!(
        Error::ProcessExit("3".into()).message(),
        "subproceso salió con código 3"
    );
    assert_eq!(
        Error::SaveConfig {
            path: "/tmp/cfg.json".into(),
            detail: "boom".into()
        }
        .message(),
        "no se pudo persistir la configuración en /tmp/cfg.json: boom"
    );
    assert_eq!(
        Error::InvalidConfig {
            key: "sync_dir".into(),
            value: "nope".into()
        }
        .message(),
        "valor de configuración inválido para 'sync_dir': nope"
    );

    // S2 — log lines, exactly as the old `format!`/literals rendered.
    assert_eq!(t("log.sync.no_stderr"), "sin stderr disponible");
    assert_eq!(t("log.sync.no_transfers"), "sin transferencias");
    assert_eq!(
        t_args("log.sync.orphan_lock_removed", &["/tmp/x.lck"]),
        "Lock huérfano eliminado: /tmp/x.lck"
    );
    assert_eq!(
        t_args("log.sync.lock_unreadable", &["/tmp/x.lck"]),
        "No se pudo leer el lock de bisync: /tmp/x.lck"
    );
    assert_eq!(
        t_args("log.daemon.sync_dir_missing", &["/nope"]),
        "sync_dir no existe: /nope"
    );
    assert_eq!(
        t_args("log.daemon.watcher_start_failed", &["boom"]),
        "no se pudo iniciar el watcher: boom"
    );
    assert_eq!(
        t_args("log.daemon.watching", &["/nope"]),
        "observando cambios en /nope"
    );
    assert_eq!(
        t_args("log.watcher.dir_missing", &["/nope"]),
        "watch dir no existe: /nope"
    );
    assert_eq!(
        t_args("log.watcher.create_failed", &["boom"]),
        "crear watcher: boom"
    );
}

#[test]
fn en_catalog_switches_with_the_same_keys() {
    let _g = guard();
    set_language("en");
    assert_eq!(
        Error::SyncTimeout.message(),
        "rclone went idle: sync aborted after stalling"
    );
    assert_eq!(
        Error::SaveConfig {
            path: "/tmp/cfg.json".into(),
            detail: "boom".into()
        }
        .message(),
        "could not persist the configuration to /tmp/cfg.json: boom"
    );
    assert_eq!(t("log.sync.no_transfers"), "no transfers");
    assert_eq!(
        t_args("log.watcher.dir_missing", &["/nope"]),
        "watch dir does not exist: /nope"
    );
    // Switching back must work in the same process (S5).
    set_language("es");
    assert_eq!(
        Error::ProcessExit("3".into()).message(),
        "subproceso salió con código 3"
    );
}

#[test]
fn os_and_subprocess_variants_stay_verbatim_and_language_independent() {
    let _g = guard();
    set_language("es");
    let io = Error::Io(std::io::Error::other("disk on fire"));
    let raw = Error::Rclone("no stdout pipe".into());
    let es = (io.message(), raw.message());
    set_language("en");
    let en = (io.message(), raw.message());
    assert_eq!(es, en, "OS/subprocess detail must not be translated");
    assert!(es.0.contains("disk on fire"), "{}", es.0);
    // `Rclone` keeps the developer prefix from `#[error("rclone: {0}")]`.
    assert_eq!(es.1, "rclone: no stdout pipe");
}

#[test]
fn display_is_english_and_ignores_the_active_language() {
    let _g = guard();
    // S6 — `Display` is developer-facing English, independent of `set_language`.
    for lang in ["es", "en"] {
        set_language(lang);
        assert_eq!(
            Error::SyncTimeout.to_string(),
            "rclone went idle: sync aborted after stalling"
        );
        assert_eq!(
            Error::ProcessExit("3".into()).to_string(),
            "subprocess exited with code 3"
        );
        assert_eq!(
            Error::InvalidConfig {
                key: "k".into(),
                value: "v".into()
            }
            .to_string(),
            "invalid configuration value for 'k': v"
        );
        assert_eq!(
            Error::SaveConfig {
                path: "/p".into(),
                detail: "d".into()
            }
            .to_string(),
            "could not persist the configuration to /p: d"
        );
    }
}

#[test]
fn unknown_language_falls_back_to_spanish() {
    let _g = guard();
    set_language("fr");
    assert_eq!(
        t("log.sync.no_transfers"),
        "sin transferencias",
        "an unsupported language must fall back to the es catalog, not leak keys"
    );
    set_language("es");
}

#[test]
fn es_and_en_expose_the_same_runtime_keys_and_no_key_leaks() {
    let _g = guard();
    let es = catalog_keys("es");
    let en = catalog_keys("en");
    assert_eq!(es, en, "catalog key sets must match");
    for key in es {
        assert_ne!(t(key), key, "{key} resolves to its own name");
    }
    // The es side of the new keys must be Spanish, not the English text (S4).
    set_language("es");
    for key in [
        "error.serde",
        "error.sync_timeout",
        "error.process_exit",
        "error.save_config",
        "error.invalid_config",
        "log.sync.no_stderr",
        "log.sync.no_transfers",
        "log.sync.orphan_lock_removed",
        "log.sync.lock_unreadable",
        "log.daemon.sync_dir_missing",
        "log.daemon.watcher_start_failed",
        "log.daemon.watching",
        "log.watcher.dir_missing",
        "log.watcher.create_failed",
    ] {
        assert_ne!(t(key), key, "missing es value for {key}");
    }
    set_language("en");
    assert_eq!(t("log.sync.no_stderr"), "no stderr available");
}
