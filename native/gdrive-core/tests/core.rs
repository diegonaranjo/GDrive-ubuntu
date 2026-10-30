//! Unit tests for the core.

#![cfg(test)]

use std::sync::Mutex;

use gdrive_core::config::{load_config, reset_config, save_config, Config};
use gdrive_core::logging::format_size;
use gdrive_core::rclone::{parse_speed, parse_stats};
use gdrive_core::sync::{interval_seconds, SyncEngine};

/// Serializes the tests that mutate `GDRIVE_CONFIG_DIR` (a global environment
/// variable shared between tests running in parallel).
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_config_defaults() {
    let c = Config::default();
    assert_eq!(c.sync_interval, "manual");
    assert_eq!(c.sync_mode, "mirror");
    assert_eq!(c.num_threads, 8);
    assert!(c.logging_enabled);
    assert!(!c.paused);
    assert_eq!(c.log_max_files, 5);
    // Junk from code trees: skipped by default (patterns validated with
    // `rclone lsf`: without the leading `**/` nested ones are not excluded).
    assert!(c
        .excluded_patterns
        .contains(&"**/node_modules/**".to_string()));
    assert!(c
        .excluded_patterns
        .contains(&"**/__pycache__/**".to_string()));
}

#[test]
fn test_config_partial_json_merges_defaults() {
    let _g = CONFIG_LOCK.lock().unwrap();
    // JSON with a single field must be filled in from the remaining defaults.
    let dir = tempfile::TempDir::new().unwrap();
    let cfg_path = dir.path().join("settings.json");
    std::fs::write(&cfg_path, r#"{"num_threads": 12}"#).unwrap();

    // Redirect config_home() via GDRIVE_CONFIG_DIR.
    std::env::set_var("GDRIVE_CONFIG_DIR", dir.path());
    let c = load_config().unwrap();
    assert_eq!(c.num_threads, 12);
    assert_eq!(c.sync_interval, "manual"); // default preserved
    assert_eq!(c.language, "es");
    std::env::remove_var("GDRIVE_CONFIG_DIR");
}

#[test]
fn test_config_roundtrip() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    std::env::set_var("GDRIVE_CONFIG_DIR", dir.path());
    reset_config().unwrap();

    let c = Config {
        sync_dir: "/tmp/x".into(),
        bandwidth_limit: "1M".into(),
        ..Config::default()
    };
    save_config(&c).unwrap();

    let loaded = load_config().unwrap();
    assert_eq!(loaded, c);
    std::env::remove_var("GDRIVE_CONFIG_DIR");
}

#[test]
fn test_format_size() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512.0 B");
    assert_eq!(format_size(2048), "2.0 KB");
    assert_eq!(format_size(3 * 1024 * 1024), "3.0 MB");
    assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.0 GB");
    // Negative values are treated as 0 (like the Python version).
    assert_eq!(format_size(-10), "0 B");
}

#[test]
fn test_parse_stats_transferred() {
    // Long rclone format.
    let l = "2026/07/06 12:00:01 INFO : Transferred: 123 / 456, 25%";
    assert_eq!(parse_stats(l), Some((123, 456)));
    // Compact format with an asterisk.
    let l2 = "2026/07/06 12:00:01 INFO : * 50 / 200, 25%";
    assert_eq!(parse_stats(l2), Some((50, 200)));
    // No stats.
    let l3 = "some unrelated line";
    assert_eq!(parse_stats(l3), None);
}

/// Format rclone v1.74 actually emits with `--stats-one-line -v`: dashed date,
/// decimal values and **different units on each side** of the `/`.
#[test]
fn test_parse_stats_one_line_normalizes_mixed_units_to_bytes() {
    let l = "2026-09-30 22:05:21 INFO  :   512 MiB / 2 GiB, 25%, 12.3 MiB/s, ETA 10s";
    assert_eq!(
        parse_stats(l),
        Some((512 * 1024 * 1024, 2 * 1024 * 1024 * 1024))
    );
}

#[test]
fn test_parse_stats_one_line_accepts_decimals() {
    let l = "2026-09-30 22:05:22 INFO  :     1.5 GiB / 3 GiB, 50%, 0 B/s, ETA -";
    assert_eq!(parse_stats(l), Some((1610612736, 3221225472)));
}

#[test]
fn test_parse_stats_one_line_zero_transfer() {
    // Real line from a run with nothing to move (NOTICE level, not INFO).
    let l = "2026-09-30 18:33:45 NOTICE:           0 B / 0 B, -, 0 B/s, ETA -";
    assert_eq!(parse_stats(l), Some((0, 0)));
}

/// With `-v` rclone emits a huge number of lines per file: none may be read as
/// stats (the user's tree has thousands because of node_modules symlinks).
#[test]
fn test_parse_stats_ignores_per_file_and_symlink_lines() {
    let copy = "2026-09-30 22:05:21 INFO  : big3.bin: Copied (new)";
    assert_eq!(parse_stats(copy), None);
    let link = "2026-09-30 18:26:50 NOTICE: Documentos/Proyectos/app/web/node_modules/react: Can't follow symlink without -L/--copy-links";
    assert_eq!(parse_stats(link), None);
}

#[test]
fn test_parse_speed_from_real_one_line() {
    let l = "2026-09-30 22:05:21 INFO  :   512 MiB / 2 GiB, 25%, 12.3 MiB/s, ETA 10s";
    assert_eq!(parse_speed(l).as_deref(), Some("12.3 MiB/s"));
}

/// Input is external output: a `/` at position 0 must not overflow indices,
/// it must return `None`.
#[test]
fn test_parse_stats_does_not_panic_on_malformed_lines() {
    assert_eq!(parse_stats("/ 512 MiB, ETA -"), None);
    assert_eq!(parse_stats("2026-09-30 22:05:21 INFO : /"), None);
    assert_eq!(parse_stats("INFO : 512 MiB /"), None);
}

/// Without verbosity rclone emits no output: progress and the inactivity
/// watchdog stay blind, so `-v` is part of the engine's contract.
#[test]
fn test_build_cmd_enables_verbose_so_stats_are_emitted() {
    let (cmd, _) = SyncEngine::new(Config::default()).build_cmd();
    assert!(cmd.iter().any(|a| a == "-v"), "{cmd:?}");
}

/// `--fast-list` changes remote listing from one call per directory to a
/// recursive pass: measured on Drive, 18.74 s -> 1.12 s on a sub-tree.
#[test]
fn test_build_cmd_uses_fast_list_to_cut_api_calls() {
    let (cmd, _) = SyncEngine::new(Config::default()).build_cmd();
    assert!(cmd.iter().any(|a| a == "--fast-list"), "{cmd:?}");
}

/// `conflict_action` governed `--conflict-resolve` in name only: the historical
/// value ("rename") does not exist in bisync. Now a valid value is propagated.
#[test]
fn test_conflict_action_maps_to_conflict_resolve() {
    for (action, esperado) in [
        ("older", "older"),
        ("larger", "larger"),
        ("path2", "path2"),
        ("none", "none"),
    ] {
        let c = Config {
            conflict_action: action.into(),
            ..Config::default()
        };
        let eng = SyncEngine::new(c);
        assert_eq!(eng.conflict_resolution(), esperado, "para {action}");
        let (cmd, _) = eng.build_cmd();
        let i = cmd
            .iter()
            .position(|a| a == "--conflict-resolve")
            .expect("--conflict-resolve debe ir presente");
        assert_eq!(cmd[i + 1], esperado);
    }
}

/// Unknown or empty values (e.g. the `"rename"` left behind in old config
/// files) must not disable conflict resolution.
#[test]
fn test_conflict_action_unknown_falls_back_to_newer() {
    for action in ["rename", "", "borrar"] {
        let c = Config {
            conflict_action: action.into(),
            ..Config::default()
        };
        assert_eq!(
            SyncEngine::new(c).conflict_resolution(),
            "newer",
            "para {action:?}"
        );
    }
}

/// rclone only applies the **last** `--bwlimit` (measured) and does not parse
/// `d:`/`u:`: emitting one flag per setting made a download limit break
/// every sync. Exactly one must be emitted, with the strictest value.
#[test]
fn test_bwlimit_emits_a_single_strictest_flag() {
    let c = Config {
        bandwidth_limit: "1M".into(),
        download_limit: "2M".into(),
        upload_limit: "500k".into(),
        ..Config::default()
    };
    let eng = SyncEngine::new(c);
    assert_eq!(eng.bwlimit_arg().as_deref(), Some("500k"));
    let (cmd, _) = eng.build_cmd();
    let n = cmd.iter().filter(|a| *a == "--bwlimit").count();
    assert_eq!(n, 1, "no debe repetirse --bwlimit: {cmd:?}");
    let i = cmd
        .iter()
        .position(|a| a == "--bwlimit")
        .expect("--bwlimit presente");
    assert_eq!(cmd[i + 1], "500k");
}

#[test]
fn test_bwlimit_absent_when_nothing_configured() {
    let (cmd, _) = SyncEngine::new(Config::default()).build_cmd();
    assert!(
        !cmd.iter().any(|a| a == "--bwlimit"),
        "no limit means no --bwlimit flag: {cmd:?}"
    );
}

/// A bare number is KiB/s according to rclone, so 1000 is stricter than 1M.
#[test]
fn test_bwlimit_bare_number_compares_as_kib() {
    let c = Config {
        bandwidth_limit: "1000".into(),
        upload_limit: "1M".into(),
        ..Config::default()
    };
    assert_eq!(SyncEngine::new(c).bwlimit_arg().as_deref(), Some("1000"));
}

/// Unparsable value: passed through as is so rclone fails loudly,
/// instead of silently syncing without a cap.
#[test]
fn test_bwlimit_unparsable_is_passed_through() {
    let c = Config {
        bandwidth_limit: "rapido".into(),
        ..Config::default()
    };
    assert_eq!(SyncEngine::new(c).bwlimit_arg().as_deref(), Some("rapido"));
}

#[test]
fn test_parse_speed() {
    // Real rclone `--stats-one-line` format: number + unit separated by a space.
    assert_eq!(
        parse_speed(
            "2026/07/06 12:00:01 INFO : Transferred: 1.5 GiB / 2.0 GiB, 75%, 15.2 MiB/s, ETA 1m"
        ),
        Some("15.2 MiB/s".to_string())
    );
    // Compact format (no space) number+unit.
    assert_eq!(
        parse_speed("2026/07/06 12:00:01 INFO : avg 800kBytes/s"),
        Some("800kBytes/s".to_string())
    );
    // A line without speed → None.
    assert_eq!(parse_speed("hola mundo"), None);
}

/// Real name rclone v1.74 gives to the listings and to the pair's lock
/// (`gdrive:` + `/home/usuario/GoogleDrive`), verified on disk.
const GOLDEN_PREFIX: &str = "gdrive_..home_usuario_GoogleDrive";

/// Writes a bisync listing with `entries` entries under `cache`.
/// The first line is the header rclone always includes.
fn write_bisync_list(cache: &std::path::Path, prefix: &str, side: &str, entries: usize) {
    let dir = cache.join("rclone").join("bisync");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = String::from("# bisync listing v1 from 2026-09-30T12:00:00+0000\n");
    for i in 0..entries {
        body.push_str(&format!(
            "=      {i} - - 2026-09-30T12:00:00+0000 \"file{i}.txt\"\n"
        ));
    }
    std::fs::write(dir.join(format!("{prefix}.{side}.lst")), body).unwrap();
}

/// Isolates a test's environment: default remote and paths under its own
/// temp dir (config and cache), restoring them on exit.
fn isolate_env(dir: &std::path::Path) -> impl FnOnce() {
    let prev_cache = std::env::var("XDG_CACHE_HOME").ok();
    let prev_config = std::env::var("GDRIVE_CONFIG_DIR").ok();
    let prev_remote = std::env::var("GDRIVE_REMOTE").ok();
    std::env::remove_var("GDRIVE_REMOTE");
    std::env::set_var("XDG_CACHE_HOME", dir);
    std::env::set_var("GDRIVE_CONFIG_DIR", dir);
    move || {
        for (k, v) in [
            ("XDG_CACHE_HOME", prev_cache),
            ("GDRIVE_CONFIG_DIR", prev_config),
            ("GDRIVE_REMOTE", prev_remote),
        ] {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// The prefix must match the file name rclone actually uses.
/// A prefix that does not match makes `has_list_files()` always `false` and
/// every sync carry `--resync`, which is `--resync-mode path1`: the remote wins
/// and overwrites local changes. That was the original bug.
#[test]
fn test_bisync_prefix_matches_real_rclone_file_names() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    let eng = SyncEngine::new(Config {
        sync_dir: "/home/usuario/GoogleDrive".into(),
        ..Config::default()
    });
    assert_eq!(eng.bisync_prefix(), GOLDEN_PREFIX);
    assert_eq!(
        eng.lock_path().file_name().unwrap().to_string_lossy(),
        format!("{GOLDEN_PREFIX}.lck")
    );
    restore();
}

/// rclone trims the leading `/` from the path and leaves no double separators.
#[test]
fn test_bisync_prefix_trims_separators_like_rclone() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    let with_slash = SyncEngine::new(Config {
        sync_dir: "/home/usuario/GoogleDrive/".into(),
        ..Config::default()
    });
    assert_eq!(with_slash.bisync_prefix(), GOLDEN_PREFIX);

    let nested = SyncEngine::new(Config {
        sync_dir: "/a/b/c".into(),
        ..Config::default()
    });
    assert_eq!(nested.bisync_prefix(), "gdrive_..a_b_c");
    restore();
}

#[test]
fn test_bisync_prefix_changes_with_sync_dir() {
    let c1 = Config {
        sync_dir: "/a/b".into(),
        ..Config::default()
    };
    let c2 = Config {
        sync_dir: "/x/y/z".into(),
        ..Config::default()
    };
    assert_ne!(
        SyncEngine::new(c1).bisync_prefix(),
        SyncEngine::new(c2).bisync_prefix()
    );
}

/// An already seeded sync must **never** carry `--resync`.
#[test]
fn test_build_cmd_seeded_sync_never_uses_resync() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    write_bisync_list(tmp.path(), GOLDEN_PREFIX, "path1", 3);
    write_bisync_list(tmp.path(), GOLDEN_PREFIX, "path2", 3);

    let eng = SyncEngine::new(Config {
        sync_dir: "/home/usuario/GoogleDrive".into(),
        ..Config::default()
    });
    assert!(eng.has_list_files());
    let (cmd, _) = eng.build_cmd();
    assert!(
        !cmd.iter().any(|a| a == "--resync"),
        "un sync sembrado no debe resyncar: {cmd:?}"
    );
    restore();
}

/// A listing with only the header is not a baseline: it is the corrupt state
/// an aborted run used to leave, and seeding is preferable to trusting it.
#[test]
fn test_build_cmd_header_only_listing_is_not_a_baseline() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    write_bisync_list(tmp.path(), GOLDEN_PREFIX, "path1", 0);
    write_bisync_list(tmp.path(), GOLDEN_PREFIX, "path2", 0);

    let eng = SyncEngine::new(Config {
        sync_dir: "/home/usuario/GoogleDrive".into(),
        ..Config::default()
    });
    assert!(!eng.has_list_files());
    let (cmd, _) = eng.build_cmd();
    assert!(cmd.iter().any(|a| a == "--resync"));
    restore();
}

#[test]
fn test_build_cmd_first_run_uses_resync() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    let c = Config {
        sync_dir: "/nonexistent/definitely/empty".into(), // no .lst files
        ..Config::default()
    };
    let eng = SyncEngine::new(c);
    let (cmd, _) = eng.build_cmd();
    assert!(cmd.iter().any(|a| a == "--resync"));
    assert!(cmd.iter().any(|a| a == "--stats-one-line"));
    // plain `--resync` = `--resync-mode path1` = the remote wins. Forbidden.
    let mode = cmd
        .iter()
        .position(|a| a == "--resync-mode")
        .map(|i| cmd[i + 1].as_str());
    assert_eq!(mode, Some("newer"));
    restore();
}

/// Partial errors (e.g. a Drive file with 403) must not leave the state
/// demanding a full resync again.
#[test]
fn test_build_cmd_recovers_without_forcing_resync() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    let (cmd, _) = SyncEngine::new(Config::default()).build_cmd();
    assert!(cmd.iter().any(|a| a == "--recover"), "{cmd:?}");
    assert!(cmd.iter().any(|a| a == "--resilient"), "{cmd:?}");
    let idx = cmd
        .iter()
        .position(|a| a == "--conflict-resolve")
        .expect("--conflict-resolve debe ir presente");
    assert_eq!(cmd[idx + 1], "newer");
    restore();
}

/// The lock of a dead process is cleaned: rclone refuses to sync
/// while it exists and the daemon would stay blocked forever.
#[test]
fn test_clear_orphan_lock_removes_lock_of_dead_pid() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    let dir = tmp.path().join("rclone").join("bisync");
    std::fs::create_dir_all(&dir).unwrap();
    let lock = dir.join(format!("{GOLDEN_PREFIX}.lck"));
    std::fs::write(
        &lock,
        r#"{"Session":"x","PID":"999999","TimeRenewed":"2026-09-30T12:00:00+0000"}"#,
    )
    .unwrap();

    SyncEngine::new(Config {
        sync_dir: "/home/usuario/GoogleDrive".into(),
        ..Config::default()
    })
    .clear_orphan_lock();
    assert!(!lock.exists());
    restore();
}

/// An unreadable lock is NOT deleted blindly: it may belong to a live session.
#[test]
fn test_clear_orphan_lock_keeps_unreadable_lock() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let restore = isolate_env(tmp.path());
    let dir = tmp.path().join("rclone").join("bisync");
    std::fs::create_dir_all(&dir).unwrap();
    let lock = dir.join(format!("{GOLDEN_PREFIX}.lck"));
    std::fs::write(&lock, "no es json").unwrap();

    SyncEngine::new(Config {
        sync_dir: "/home/usuario/GoogleDrive".into(),
        ..Config::default()
    })
    .clear_orphan_lock();
    assert!(lock.exists());
    restore();
}

#[test]
fn test_build_cmd_applies_bandwidth_and_threads() {
    let c = Config {
        bandwidth_limit: "2M".into(),
        num_threads: 16,
        ..Config::default()
    };
    let eng = SyncEngine::new(c);
    let (cmd, _) = eng.build_cmd();
    let idx = cmd.iter().position(|a| a == "--bwlimit").unwrap();
    assert_eq!(cmd[idx + 1], "2M");
    let tidx = cmd.iter().position(|a| a == "--transfers").unwrap();
    assert_eq!(cmd[tidx + 1], "16");
}

#[test]
fn test_build_cmd_applies_excludes_and_proxy() {
    let c = Config {
        excluded_patterns: vec!["*.tmp".into()],
        skip_hidden: true,
        proxy_enabled: true,
        proxy_url: "socks5://127.0.0.1:1080".into(),
        ..Config::default()
    };
    let eng = SyncEngine::new(c);
    let (cmd, _) = eng.build_cmd();
    assert!(cmd
        .iter()
        .any(|a| a == "--exclude" && cmd[cmd.iter().position(|x| x == a).unwrap() + 1] == "*.tmp"));
    assert!(cmd.iter().any(|a| a == "--proxy"));
}

#[test]
fn test_sync_mode_mirror_has_no_includes() {
    // `mirror` (default) syncs the whole tree: no `--include`.
    let c = Config {
        sync_mode: "mirror".into(),
        ..Config::default()
    };
    let (cmd, _) = SyncEngine::new(c).build_cmd();
    assert!(!cmd.iter().any(|a| a == "--include"));
}

#[test]
fn test_sync_mode_selective_adds_includes() {
    // `selective` restricts the sync to the selected folders.
    let c = Config {
        sync_mode: "selective".into(),
        selected_folders: vec!["Docs".into(), "Photos/".into()],
        ..Config::default()
    };
    let eng = SyncEngine::new(c);
    let includes = eng.selective_includes();
    assert_eq!(
        includes,
        vec!["Docs/**".to_string(), "Photos/**".to_string()]
    );

    let (cmd, _) = eng.build_cmd();
    // It must contain both `--include` flags with their values.
    let mut seen = Vec::new();
    let mut i = 0;
    while i < cmd.len() {
        if cmd[i] == "--include" {
            seen.push(cmd[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    assert_eq!(seen, vec!["Docs/**".to_string(), "Photos/**".to_string()]);
}

#[test]
fn test_sync_mode_selective_empty_folders_no_includes() {
    let c = Config {
        sync_mode: "selective".into(),
        selected_folders: vec![],
        ..Config::default()
    };
    let (cmd, _) = SyncEngine::new(c).build_cmd();
    assert!(!cmd.iter().any(|a| a == "--include"));
}

#[test]
fn test_legacy_stream_mode_behaves_like_mirror() {
    // `stream` is no longer offered in the UI because the engine does not
    // implement it, but it can come saved in an old settings.json: it must
    // behave like `mirror` (no filters) and not start a sync with `--include`.
    let c = Config {
        sync_mode: "stream".into(),
        ..Config::default()
    };
    let (cmd, _) = SyncEngine::new(c).build_cmd();
    assert!(!cmd.iter().any(|a| a == "--include"));
}

#[test]
fn test_watch_changes_off_by_default() {
    // Real-time change detection must be disabled by default.
    assert!(!Config::default().watch_changes);
}

#[test]
fn test_interval_seconds() {
    assert_eq!(interval_seconds("manual"), None);
    assert_eq!(interval_seconds("5min"), Some(300));
    assert_eq!(interval_seconds("15min"), Some(900));
    assert_eq!(interval_seconds("30min"), Some(1800));
    assert_eq!(interval_seconds("1hr"), Some(3600));
    assert_eq!(interval_seconds("bogus"), None);
}

#[test]
fn test_i18n_es_en() {
    gdrive_core::i18n::set_language("es");
    assert_eq!(gdrive_core::i18n::t("status.idle"), "Listo");
    gdrive_core::i18n::set_language("en");
    assert_eq!(gdrive_core::i18n::t("status.idle"), "Ready");
    // Missing key → returns the key itself.
    assert_eq!(gdrive_core::i18n::t("no.existe"), "no.existe");
}

#[test]
fn test_constants() {
    assert_eq!(gdrive_core::APP_ID, "com.gdrive.Client");
    assert!(!gdrive_core::APP_VERSION.is_empty());
    // The default rclone_remote is "gdrive:".
    assert_eq!(gdrive_core::constants::rclone_remote(), "gdrive:");
}

#[test]
fn test_run_with_callback_receives_progress() {
    let _g = CONFIG_LOCK.lock().unwrap();
    // Without isolation, log_write uses ~/.config/gdrive and the test writes
    // fake sync lines into the user's real log.
    let cache = tempfile::TempDir::new().unwrap();
    // Creates a fake `rclone` in a temp dir that emits stats and speed,
    // to verify the callback contract without depending on real rclone.
    // Note: rclone emits logs and `--stats` on **stderr** and leaves stdout
    // empty; the fake replicates that contract, so this is also the regression
    // test for reading the wrong stream.
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("rclone");
    let script = "#!/bin/sh\n\
echo '2026-09-30 22:05:21 INFO  :   512 MiB / 2 GiB, 25%, 12.3 MiB/s, ETA 10s' >&2\n\
echo 'Some other line' >&2\n";
    std::fs::write(&bin, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let restore = isolate_env(cache.path());
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", tmp.path().display(), orig_path.to_string_lossy());
    std::env::set_var("PATH", new_path);

    let cfg = Config {
        sync_dir: tmp.path().join("gdrive-sync").to_string_lossy().to_string(),
        ..Config::default()
    };
    let eng = SyncEngine::new(cfg);

    let mut seen_transferred = 0;
    let mut seen_total = 0;
    let mut seen_speed = false;
    let res = eng.run_with_callback(|_line, tr, to, speed| {
        seen_transferred = tr;
        seen_total = to;
        if speed.is_some() {
            seen_speed = true;
        }
    });

    assert!(
        res.is_ok(),
        "run_with_callback must not fail with the fake rclone: {:?}",
        res.err()
    );
    // The real format uses MiB/GiB: what the callback delivers are BYTES.
    assert_eq!(seen_transferred, 512 * 1024 * 1024);
    assert_eq!(seen_total, 2 * 1024 * 1024 * 1024);
    assert!(seen_speed, "the callback should report speed");

    std::env::set_var("PATH", orig_path);
    restore();
}

/// A failed run must include rclone's ERROR line in the message: the detail
/// comes from the stderr `tail` (already consumed in streaming), not from a
/// second pipe that was never read during the run.
#[test]
fn test_run_reports_failure_detail_from_stderr_lines() {
    let _g = CONFIG_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let restore = isolate_env(tmp.path());
    let bin = tmp.path().join("rclone");
    std::fs::write(
        &bin,
        "#!/bin/sh\necho '2026-09-30 15:19:52 ERROR : Bisync critical error: cannotDownloadAbusiveFile' >&2\nexit 7\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("{}:{}", tmp.path().display(), orig_path.to_string_lossy()),
    );

    let eng = SyncEngine::new(Config {
        sync_dir: tmp.path().join("gdrive-sync").to_string_lossy().to_string(),
        ..Config::default()
    });
    let out = eng
        .run_with_callback(|_l, _tr, _to, _spd| {})
        .expect("el fake debe ejecutarse");

    std::env::set_var("PATH", orig_path);
    restore();

    assert!(!out.ok, "the fake exits with code 7");
    assert!(
        out.message.contains("cannotDownloadAbusiveFile"),
        "the message lost rclone's detail: {}",
        out.message
    );
}
