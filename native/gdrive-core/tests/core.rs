//! Tests unitarios del core.

#![cfg(test)]

use std::sync::Mutex;

use gdrive_core::config::{load_config, reset_config, save_config, Config};
use gdrive_core::logging::format_size;
use gdrive_core::rclone::{parse_speed, parse_stats};
use gdrive_core::sync::{interval_seconds, SyncEngine};

/// Serializa los tests que mutan `GDRIVE_CONFIG_DIR` (variable de entorno global
/// compartida entre tests en paralelo).
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
    // Basura de trees de código: se omite por defecto (patrones validados con
    // `rclone lsf`: sin el `**/` delantero los anidados no se excluyen).
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
    // Un JSON con un solo campo debe completarse con los defaults restantes.
    let dir = tempfile::TempDir::new().unwrap();
    let cfg_path = dir.path().join("settings.json");
    std::fs::write(&cfg_path, r#"{"num_threads": 12}"#).unwrap();

    // Redirigir config_home() vía GDRIVE_CONFIG_DIR.
    std::env::set_var("GDRIVE_CONFIG_DIR", dir.path());
    let c = load_config().unwrap();
    assert_eq!(c.num_threads, 12);
    assert_eq!(c.sync_interval, "manual"); // default preservado
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
    // Negativo se trata como 0 (como el Python).
    assert_eq!(format_size(-10), "0 B");
}

#[test]
fn test_parse_stats_transferred() {
    // Formato largo de rclone.
    let l = "2026/07/06 12:00:01 INFO : Transferred: 123 / 456, 25%";
    assert_eq!(parse_stats(l), Some((123, 456)));
    // Formato compacto con asterisco.
    let l2 = "2026/07/06 12:00:01 INFO : * 50 / 200, 25%";
    assert_eq!(parse_stats(l2), Some((50, 200)));
    // Sin stats.
    let l3 = "some unrelated line";
    assert_eq!(parse_stats(l3), None);
}

/// Formato que rclone v1.74 emite realmente con `--stats-one-line -v`: fecha con
/// guiones, valores decimales y **unidades distintas a cada lado** del `/`.
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
    // Línea real de un run sin nada que mover (nivel NOTICE, no INFO).
    let l = "2026-09-30 18:33:45 NOTICE:           0 B / 0 B, -, 0 B/s, ETA -";
    assert_eq!(parse_stats(l), Some((0, 0)));
}

/// Con `-v` rclone emite muchísimas líneas por archivo: ninguna debe leerse como
/// stats (el árbol del usuario tiene miles por los enlaces de node_modules).
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

/// La entrada es salida externa: un `/` en posición 0 no debe desbordar índices,
/// debe devolver `None`.
#[test]
fn test_parse_stats_does_not_panic_on_malformed_lines() {
    assert_eq!(parse_stats("/ 512 MiB, ETA -"), None);
    assert_eq!(parse_stats("2026-09-30 22:05:21 INFO : /"), None);
    assert_eq!(parse_stats("INFO : 512 MiB /"), None);
}

/// Sin verbosity rclone no emite salida: el progreso y el watchdog de inactividad
/// se quedan ciegos, así que `-v` es parte del contrato del motor.
#[test]
fn test_build_cmd_enables_verbose_so_stats_are_emitted() {
    let (cmd, _) = SyncEngine::new(Config::default()).build_cmd();
    assert!(cmd.iter().any(|a| a == "-v"), "{cmd:?}");
}

/// `--fast-list` cambia el listado del remoto de una llamada por directorio a una
/// pasada recursiva: medido en Drive, 18,74 s -> 1,12 s en un subárbol.
#[test]
fn test_build_cmd_uses_fast_list_to_cut_api_calls() {
    let (cmd, _) = SyncEngine::new(Config::default()).build_cmd();
    assert!(cmd.iter().any(|a| a == "--fast-list"), "{cmd:?}");
}

/// `conflict_action` gobernaba `--conflict-resolve` solo de nombre: el valor
/// histórico ("rename") no existe en bisync. Ahora un valor válido se propaga.
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

/// Valores desconocidos o vacíos (p. ej. el `"rename"` que quedó en archivos de
/// configuración viejos) no pueden desactivar la resolución de conflictos.
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

/// rclone solo aplica el **último** `--bwlimit` (medido) y no entiende `d:`/`u:`:
/// emitir un flag por cada ajuste hacía que poner límite de descarga rompiera
/// cada sincronización. Debe salir exactamente uno, con el valor más estricto.
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
        "sin límites no va --bwlimit: {cmd:?}"
    );
}

/// Un número suelto es KiB/s según rclone, así que 1000 es más estricto que 1M.
#[test]
fn test_bwlimit_bare_number_compares_as_kib() {
    let c = Config {
        bandwidth_limit: "1000".into(),
        upload_limit: "1M".into(),
        ..Config::default()
    };
    assert_eq!(SyncEngine::new(c).bwlimit_arg().as_deref(), Some("1000"));
}

/// Valor no parseable: se propaga tal cual para que rclone falle en voz alta,
/// en lugar de sincronizar sin cap en silencio.
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
    // Formato real `--stats-one-line` de rclone: número + unidad separados por espacio.
    assert_eq!(
        parse_speed(
            "2026/07/06 12:00:01 INFO : Transferred: 1.5 GiB / 2.0 GiB, 75%, 15.2 MiB/s, ETA 1m"
        ),
        Some("15.2 MiB/s".to_string())
    );
    // Formato compacto (sin espacio) número+unidad.
    assert_eq!(
        parse_speed("2026/07/06 12:00:01 INFO : avg 800kBytes/s"),
        Some("800kBytes/s".to_string())
    );
    // Una línea sin velocidad → None.
    assert_eq!(parse_speed("hola mundo"), None);
}

/// Nombre real que rclone v1.74 da a los listados y al lock del par
/// (`gdrive:` + `/home/usuario/GoogleDrive`), verificado en disco.
const GOLDEN_PREFIX: &str = "gdrive_..home_usuario_GoogleDrive";

/// Escribe un listado de bisync con `entries` entradas bajo `cache`.
/// La primera línea es la cabecera que rclone pone siempre.
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

/// Aísla el entorno de un test: remote por defecto y rutas bajo un temp dir
/// propio (config y caché), restaurándolos al salir.
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

/// El prefijo debe coincidir con el nombre de archivo que realmente usa rclone.
/// Un prefijo que no coincide hace que `has_list_files()` sea siempre `false` y
/// que todo sync lleve `--resync`, que es `--resync-mode path1`: el remoto gana
/// y pisa los cambios locales. Era el bug original.
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

/// rclone recorta el `/` inicial de la ruta y no deja dobles separadores.
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

/// Un sync ya sembrado **nunca** debe llevar `--resync`.
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

/// Un listado con solo la cabecera no es una línea base: es el estado corrupto
/// que dejaba un run abortado, y sembrarlo es preferible a confiar en él.
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
        sync_dir: "/nonexistent/definitely/empty".into(), // sin .lst
        ..Config::default()
    };
    let eng = SyncEngine::new(c);
    let (cmd, _) = eng.build_cmd();
    assert!(cmd.iter().any(|a| a == "--resync"));
    assert!(cmd.iter().any(|a| a == "--stats-one-line"));
    // `--resync` a secas = `--resync-mode path1` = gana el remoto. Prohibido.
    let mode = cmd
        .iter()
        .position(|a| a == "--resync-mode")
        .map(|i| cmd[i + 1].as_str());
    assert_eq!(mode, Some("newer"));
    restore();
}

/// Errores parciales (p. ej. un archivo de Drive con 403) no deben volver a
/// dejar el estado exigiendo un resync completo.
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

/// El lock de un proceso muerto sí se limpia: rclone se niega a sincronizar
/// mientras exista y el daemon quedaría bloqueado para siempre.
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

/// Un lock ilegible NO se borra a ciegas: podría ser de una sesión viva.
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
    // `mirror` (default) sincroniza todo el árbol: sin `--include`.
    let c = Config {
        sync_mode: "mirror".into(),
        ..Config::default()
    };
    let (cmd, _) = SyncEngine::new(c).build_cmd();
    assert!(!cmd.iter().any(|a| a == "--include"));
}

#[test]
fn test_sync_mode_selective_adds_includes() {
    // `selective` restringe el sync a las carpetas seleccionadas.
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
    // Debe contener los dos `--include` con sus valores.
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
    // `stream` ya no se ofrece en la UI porque el motor no lo implementa, pero
    // puede venir guardado en un settings.json viejo: debe comportarse como
    // `mirror` (sin filtros) y no abrir un sync con `--include`.
    let c = Config {
        sync_mode: "stream".into(),
        ..Config::default()
    };
    let (cmd, _) = SyncEngine::new(c).build_cmd();
    assert!(!cmd.iter().any(|a| a == "--include"));
}

#[test]
fn test_watch_changes_off_by_default() {
    // La detección en tiempo real debe estar desactivada por defecto.
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
    // Clave inexistente → devuelve la propia clave.
    assert_eq!(gdrive_core::i18n::t("no.existe"), "no.existe");
}

#[test]
fn test_constants() {
    assert_eq!(gdrive_core::APP_ID, "com.gdrive.Client");
    assert!(!gdrive_core::APP_VERSION.is_empty());
    // rclone_remote default es "gdrive:".
    assert_eq!(gdrive_core::constants::rclone_remote(), "gdrive:");
}

#[test]
fn test_run_with_callback_receives_progress() {
    let _g = CONFIG_LOCK.lock().unwrap();
    // Sin aislamiento, log_write usa ~/.config/gdrive y el test escribe líneas
    // falsas de sync en el log real del usuario.
    let cache = tempfile::TempDir::new().unwrap();
    // Crea un `rclone` fake en un dir temporal que emite stats y velocidad,
    // para verificar el contrato del callback sin depender de rclone real.
    // Ojo: rclone emite logs y `--stats` por **stderr** y deja stdout vacío; el
    // falso replica ese contrato, así que esto es también la regresión de leer
    // el stream equivocado.
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
        "run_with_callback no debería fallar con rclone fake: {:?}",
        res.err()
    );
    // El formato real trae MiB/GiB: lo que el callback entrega son BYTES.
    assert_eq!(seen_transferred, 512 * 1024 * 1024);
    assert_eq!(seen_total, 2 * 1024 * 1024 * 1024);
    assert!(seen_speed, "el callback debería reportar velocidad");

    std::env::set_var("PATH", orig_path);
    restore();
}

/// Un run fallido debe traer en el mensaje la línea ERROR de rclone: el detalle
/// se saca del `tail` de stderr (que ya se consume en streaming), no de un
/// segundo pipe que nunca se leía durante el run.
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

    assert!(!out.ok, "el fake sale con código 7");
    assert!(
        out.message.contains("cannotDownloadAbusiveFile"),
        "el mensaje perdió el detalle de rclone: {}",
        out.message
    );
}
