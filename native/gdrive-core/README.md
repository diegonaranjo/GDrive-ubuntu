# gdrive-core

Native **Rust** core of the **GDrive** desktop client (Google Drive for
Ubuntu). It reimplements the orchestration layer that lived in Python
(`gdrive/config.py`, `gdrive/helpers.py`, `gdrive/i18n.py` and the legacy
`gdrive/sync.py`, removed in 1.1.0) with safe types, explicit error handling
(`Result`) and real tests.

> The network work (upload/download/Google authentication) is still done by
> **`rclone`** as a subprocess. This crate builds commands, manages state and
> parses output; it does not reimplement the transfer.

## Hybrid architecture

```
┌──────────────────────────────────────────────────────────┐
│ gdrive-core (Rust, this crate) — the engine               │
│   config · sync (rclone bisync) · logging · i18n · rclone │
└───────────────────────────┬──────────────────────────────┘
                            │ D-Bus (later phase)
 ┌──────────────────────────┴───────────────────────────────┐
│ UI GTK4/libadwaita (Python PyGObject) — window + pages    │
│ tray GTK3/AppIndicator3 (Python) — tray icon              │
└───────────────────────────────────────────────────────────┘
```

The current GTK4 UI is functional and attractive; the `gtk4-rs` ecosystem is
stagnant, so the recommended migration keeps the UI in Python and moves the
logic into the Rust core, communicating over D-Bus.

## Modules

| Module | Responsibility | Python equivalent |
|---|---|---|
| `config` | Typed `Config` (serde), atomic load/save, defaults | `gdrive/config.py` |
| `constants` | APP_ID, version, remote, paths | `gdrive/constants.py` |
| `i18n` | ES/EN translations + per-file override | `gdrive/i18n.py` |
| `logging` | Log with rotation + activity log (a single implementation) | `gdrive/helpers.py` |
| `rclone` | Subprocess wrappers + stats parsing | `gdrive/helpers.py`, legacy `sync.py` |
| `sync` | `SyncEngine` (bisync, lock, resync, timeout) | `gdrive/sync.py` (removed in 1.1.0) |

## Fixes relative to the Python version

1. **bisync lock/`.lst` derived from `sync_dir`** (bug #1 of the analysis): a
   hardcoded prefix with a concrete path from the author's disk is no longer
   used.
2. **The whole config is applied to the command** (bug #2): `download_limit`,
   `upload_limit`, `preserve_timestamps`, `delete_local`, `proxy`, on top of
   what was already being used.
3. **No shared mutable state and no `GLib.idle_add`**: each `run()` is
   self-contained and returns `Result<SyncOutcome>`, with no race conditions.
4. **Unified logging** (bug #7): a single implementation used by the core.
5. **`config` loading that reports errors** instead of swallowing exceptions.

## Build and test

Requires a stable Rust toolchain with `rustfmt` and `clippy`:

```bash
rustup default stable      # installs rustc + cargo + rustfmt + clippy (1.98+)
cd native/gdrive-core

cargo fmt --all -- --check # formatting
cargo clippy --all-targets --all-features --locked -- -D warnings  # linting
cargo test                 # unit tests (19 tests)
cargo build --release      # produces target/release/gdrive-daemon
```

> The GitHub Actions CI (`.github/workflows/ci.yml`) runs exactly these
> steps with `-D warnings`, so a PR must pass clippy green.

Smoke-test CLI:

```bash
gdrive-core version        # 1.1.0
gdrive-core authorized     # true/false (valid token)
gdrive-core email          # email of the connected account
gdrive-core about          # storage JSON
```

## D-Bus daemon (Phase 2)

The `gdrive-daemon` binary exposes the engine to the UI over D-Bus, registering
the name `com.gdrive.Client` (compatible with the current tray icon).

```bash
gdrive-daemon              # foreground (debug)
gdrive-daemon --once       # one manual sync, then exit
```

### Interfaces served at `/com/gdrive/Client`

| Interface | Methods | Purpose |
|---|---|---|
| `com.gdrive.Daemon1` | `SyncNow`, `Pause`, `Resume`, `GetStatus`, `GetProgress`, `UpdateConfig`, `Quit` | main API for the UI |
| `org.freedesktop.Application` | `ActivateAction`, `Activate`, `Open` | compatibility with `gapplication action` |
| `org.gtk.Actions` | `List`, `Activate` | GTK ActionMap |

Signals emitted by `com.gdrive.Daemon1`:

| Signal | Signature | When |
|---|---|---|
| `StatusChanged` | `(s b)` | state change (`idle`/`syncing`/`paused`/`error` + `paused`) |
| `Progress` | `(s t t)` | per rclone stats line: text, transferred, total |
| `SpeedChanged` | `(s)` | transfer speed change (or `"—"`) |
| `SyncFinished` | `(b s)` | end of sync: ok + message |

`GetProgress` returns `(t t s)` = transferred, total, current speed.

### Testing the daemon

```bash
gdrive-daemon &
busctl --user call com.gdrive.Client /com/gdrive/Client com.gdrive.Daemon1 GetStatus
busctl --user call com.gdrive.Client /com/gdrive/Client com.gdrive.Daemon1 SyncNow
gapplication action com.gdrive.Client sync-now   # same effect via actions
busctl --user call com.gdrive.Client /com/gdrive/Client com.gdrive.Daemon1 Quit
```

`SyncNow` is **asynchronous**: it answers `started`/`already_syncing` right away
and the sync runs on a separate thread, emitting `Progress`/`SpeedChanged` during
the transfer and `StatusChanged` + `SyncFinished` when it finishes.

### Periodic synchronization

The daemon starts a background scheduler that respects the explicit-control
rule: it synchronizes automatically **only** if `sync_interval` in the config
differs from `manual` (values `5min`/`15min`/`30min`/`1hr`) and the sync is not
paused. It never overlaps two synchronizations (it skips the tick if one is
already running). With `manual` it re-evaluates the config every 30 s in case it
changes.

## Status

- [x] Phase 1 — standalone `gdrive-core` + tests
- [x] Phase 2 — D-Bus daemon (`zbus`) exposing status, progress and `sync-now`
- [x] Phase 3 — Python GTK4 UI wired to the daemon (D-Bus client + progress + periodic sync)
- [ ] Phase 4 — `.deb` packaging (native binary + assets)

## License

MIT. See `LICENSE` in the repository root.