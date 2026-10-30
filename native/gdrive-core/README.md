# gdrive-core

Núcleo nativo en **Rust** del cliente de escritorio **GDrive** (Google Drive para
Ubuntu). Reimplementa la capa de orquestación que hoy vive en Python
(`gdrive/sync.py`, `gdrive/config.py`, `gdrive/helpers.py`, `gdrive/i18n.py`)
con tipos seguros, manejo de errores explícito (`Result`) y tests reales.

> El trabajo de red (subir/bajar/autenticación Google) lo sigue haciendo
> **`rclone`** como subproceso. Este crate arma comandos, gestiona estado y
> parsea salida; no reimplementa la transferencia.

## Arquitectura híbrida

```
┌──────────────────────────────────────────────────────────┐
│ gdrive-core (Rust, este crate) — el motor                 │
│   config · sync (rclone bisync) · logging · i18n · rclone │
└───────────────────────────┬──────────────────────────────┘
                            │ D-Bus (fase posterior)
 ┌──────────────────────────┴───────────────────────────────┐
│ UI GTK4/libadwaita (Python PyGObject) — ventana + páginas │
│ tray GTK3/AppIndicator3 (Python) — icono de bandeja       │
└───────────────────────────────────────────────────────────┘
```

La UI GTK4 actual es funcional y atractiva; el ecosistema `gtk4-rs` está
estancado, así que la migración recomendada mantiene la UI en Python y mueve la
lógica al core Rust, comunicándose por D-Bus.

## Módulos

| Módulo | Responsabilidad | Equivalente Python |
|---|---|---|
| `config` | `Config` tipado (serde), load/save atómico, defaults | `gdrive/config.py` |
| `constants` | APP_ID, versión, remote, rutas | `gdrive/constants.py` |
| `i18n` | Traducciones ES/EN + override por archivo | `gdrive/i18n.py` |
| `logging` | Log con rotación + activity log (una sola implementación) | `gdrive/helpers.py` |
| `rclone` | Wrappers de subprocesos + parseo de stats | `gdrive/helpers.py`, `sync.py` |
| `sync` | `SyncEngine` (bisync, lock, resync, timeout) | `gdrive/sync.py` |

## Correcciones frente al Python

1. **Lock/`.lst` de bisync derivado de `sync_dir`** (bug #1 del análisis): ya no
   se usa un prefijo hardcodeado con una ruta concreta del disco del autor.
2. **Toda la config se aplica al comando** (bug #2): `download_limit`,
   `upload_limit`, `preserve_timestamps`, `delete_local`, `proxy`, además de lo
   que ya se usaba.
3. **Sin estado compartido mutable ni `GLib.idle_add`**: cada `run()` es
   autocontenida y devuelve `Result<SyncOutcome>`, sin condiciones de carrera.
4. **Logging unificado** (bug #7): una sola implementación usada por core.
5. **`config` con carga que reporta errores** en vez de tragarse excepciones.

## Compilar y testear

Requiere un toolchain de Rust estable con `rustfmt` y `clippy`:

```bash
rustup default stable      # instala rustc + cargo + rustfmt + clippy (1.98+)
cd native/gdrive-core

cargo fmt --all -- --check # formato
cargo clippy --all-targets --all-features --locked -- -D warnings  # linting
cargo test                 # tests unitarios (19 tests)
cargo build --release      # genera target/release/gdrive-daemon
```

> El CI de GitHub Actions (`.github/workflows/ci.yml`) ejecuta exactamente estos
> pasos con `-D warnings`, así que un PR debe pasar el clippy en verde.

CLI de smoke test:

```bash
gdrive-core version        # 1.1.0
gdrive-core authorized     # true/false (token válido)
gdrive-core email          # email de la cuenta conectada
gdrive-core about          # JSON de almacenamiento
```

## Daemon D-Bus (Fase 2)

El binario `gdrive-daemon` expone el motor a la UI mediante D-Bus, registrando el
nombre `com.gdrive.Client` (compatible con el tray icon actual).

```bash
gdrive-daemon              # primer plano (debug)
gdrive-daemon --once       # un sync manual y sale
```

### Interfaces servidas en `/com/gdrive/Client`

| Interfaz | Métodos | Propósito |
|---|---|---|
| `com.gdrive.Daemon1` | `SyncNow`, `Pause`, `Resume`, `GetStatus`, `GetProgress`, `UpdateConfig`, `Quit` | API principal para la UI |
| `org.freedesktop.Application` | `ActivateAction`, `Activate`, `Open` | compatibilidad con `gapplication action` |
| `org.gtk.Actions` | `List`, `Activate` | ActionMap de GTK |

Señales emitidas por `com.gdrive.Daemon1`:

| Señal | Firma | Cuándo |
|---|---|---|
| `StatusChanged` | `(s b)` | cambio de estado (`idle`/`syncing`/`paused`/`error` + `paused`) |
| `Progress` | `(s t t)` | por línea de stats de rclone: texto, transferido, total |
| `SpeedChanged` | `(s)` | cambio de velocidad de transferencia (o `"—"`) |
| `SyncFinished` | `(b s)` | fin de sync: ok + mensaje |

`GetProgress` devuelve `(t t s)` = transferido, total, velocidad actual.

### Probar el daemon

```bash
gdrive-daemon &
busctl --user call com.gdrive.Client /com/gdrive/Client com.gdrive.Daemon1 GetStatus
busctl --user call com.gdrive.Client /com/gdrive/Client com.gdrive.Daemon1 SyncNow
gapplication action com.gdrive.Client sync-now   # mismo efecto vía actions
busctl --user call com.gdrive.Client /com/gdrive/Client com.gdrive.Daemon1 Quit
```

`SyncNow` es **asíncrono**: responde `started`/`already_syncing` de inmediato y el
sync corre en un hilo aparte, emitiendo `Progress`/`SpeedChanged` durante la
transferencia y `StatusChanged` + `SyncFinished` al finalizar.

### Sincronización periódica

El daemon arranca un programador en segundo plano que respeta la regla de
control explícito: **solo** sincroniza automáticamente si `sync_interval` en la
config es distinto de `manual` (valores `5min`/`15min`/`30min`/`1hr`) y el sync
no está pausado. Nunca solapa dos sincronizaciones (salta el tick si ya hay una
en curso). Con `manual` re-evalúa la config cada 30 s por si cambia.

## Estado

- [x] Fase 1 — `gdrive-core` standalone + tests
- [x] Fase 2 — daemon D-Bus (`zbus`) que expone estado, progreso y `sync-now`
- [x] Fase 3 — UI GTK4 Python acoplada al daemon (cliente D-Bus + progreso + sync periódico)
- [ ] Fase 4 — empaquetado `.deb` (binario nativo + assets)

## Licencia

MIT. Ver `LICENSE` en el repositorio raíz.