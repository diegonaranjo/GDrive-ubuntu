# Plan: UI Refactor and Reformat — Migrate to Native Rust GTK4

> Status: **Approved — Phase 1 in progress**.
> Date: 2026-09-06
> Target: GDrive v2.0 (post the v1.0 Python UI)
> Branch: `feature/rust-ui`

## 1. Context and Problem

GDrive's desktop client currently ships three UI layers built on top of a
well-factored Rust core (`native/gdrive-core`):

| Component | Technology | LOC | Constraint |
|---|---|---|---|
| Main window (6 pages) | GTK4 / libadwaita + PyGObject | ~2,700 | — |
| Tray icon | GTK3 + AyatanaAppIndicator3 | ~480 | Cannot coexist with GTK4 in one process |
| Core (sync, watcher, config, D-Bus daemon) | Rust | existing | Reused unchanged |

The Rust core already does the heavy lifting (rclone bisync orchestration,
inotify change detection, configuration, logging, i18n strings are duplicated in
Python, D-Bus daemon). The UI "skin" remains in Python, which brings measurable
costs for a vanguard desktop client:

- **GIL**: single-threaded UI/render interactions; concurrency is awkward.
- **Memory**: ~30–45 MB of interpreter overhead per process.
- **Startup**: ~200–400 ms slower than a native equivalent.
- **Two runtimes**: Python packages (gi, AyatanaAppIndicator3) must be
  installed and version-matched with the GTK stack.

This document proposes migrating the UI to native Rust while preserving the
product's visual identity (libadwaita) and reusing the existing Rust core.

## 2. Goals and Non-Goals

### Goals

1. Native performance: Rust UI with no GIL, lower memory, faster startup.
2. Preserve libadwaita look-and-feel (the most native/elegant option for Ubuntu
   GNOME).
3. Reuse the existing `native/gdrive-core` crate (sync engine, watcher,
   config, logging, i18n keys).
4. Modern tray integration via the StatusNotifierItem protocol.
5. A clean, phased cutover that never leaves the user without a working client.

### Non-Goals

- Rewriting the sync engine or bisync logic (out of scope; core is stable).
- Migrating to a non-native/webview UI (Flutter, Tauri) in this iteration.
- Dropping Spanish/English i18n (existing convention is preserved).

## 3. Options Considered

| Option | Performance | Native elegance | Linux + tray maturity | Rewrite cost |
|---|---|---|---|---|
| **A. GTK4 in Rust (gtk4-rs + relm4)** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ (libadwaita) | Medium (tray via `tray-icon`) | High |
| **B. Flutter (Dart)** | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ (own, not native) | Medium (tray via FFI/plugin) | High, loses libadwaita |
| **C. Qt6/QML + Kirigami** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ (best on KDE) | Medium | High, foreign on GNOME |
| **D. Tauri (webview + Rust)** | ⭐⭐⭐⭐ | ⭐⭐⭐ (HTML, not native) | Full (tray-icon crate) | Medium-high |

### Decision

**Option A — GTK4 in Rust (gtk4-rs), phased.** It is the only path that upgrades
the runtime (Rust, no GIL) while keeping the exact libadwaita visual identity,
and it is consistent with the existing Rust toolchain (rustup, clippy, thiserror,
CI). The tray is migrated separately to the modern `tray-icon` crate
(StatusNotifierItem).

## 4. Architecture (target state)

```
gdrive-ui (new Rust crate)            gdrive-core (existing)
──────────────────────────            ─────────────────────────
gtk4 + libadwaita window              sync engine (rclone bisync)
relm4 (optional reactive layer)       inotify watcher
D-Bus client (Daemon1)  ───────────▶  config / logging / i18n keys
tray-icon (StatusNotifierItem)        D-Bus daemon (com.gdrive.Client)
```

Two integration strategies for UI ↔ core (decision pending, see §7):

- **D-Bus contract (lower risk)**: the Rust UI keeps talking to the daemon over
  the existing `com.gdrive.Daemon1` interface — byte-for-byte the same contract
  the Python UI uses today. The daemon and tray stay untouched during the
  transition.
- **In-process crate (max performance)**: the UI links `gdrive-core` as a
  library and calls the sync engine directly, removing the D-Bus hop for the UI.
  The daemon/tray still use D-Bus for their own processes.

## 5. Phased Roadmap

### Phase 0 — Preparation and decisions

- [x] Decide UI↔core integration: **D-Bus contract** (see §7).
- [x] Freeze the current D-Bus interface as the reference spec.
- [x] Create branch `feature/rust-ui` (never touch `main`).
- [x] Install system prerequisite (pending): `libgtk-4-dev libadwaita-1-dev`.

### Phase 1 — GTK4 Rust skeleton (window + navigation)

- [x] New crate `gdrive-ui` at `native/gdrive-ui` (bin `gdrive-ui`).
- [x] Port `window.py` shell: sidebar (6 items) + page container + navigation +
      `ToastOverlay`/`ToolbarView`/`HeaderBar` (`src/window.rs`).
- [x] Port `theme.py` baseline (default Adwaita; custom CSS deferred).
- [x] Port `daemon_client.py` → `src/dbus_client.rs` (GetStatus/SyncNow/Pause/
      Resume/UpdateConfig + `Status` model + unit tests).
- [x] Compile-verify: `cargo check`/`clippy -D warnings`/`test`/`build --release`
      all green (gtk4 0.11.4, libadwaita 0.9.2, glib 0.22.9 — latest available).
- [x] Wire the real Status page (done in Phase 2).

### Phase 2 — Pages (increasing complexity)

- [x] **Status** page (~145 LOC) — proof of concept, live: badge, title/detail,
      progress bar, info rows, Sync/Pause buttons, D-Bus signal subscriptions.
      Switched `dbus_client` from `zbus`/`tokio` to GIO (idiomatic GTK main-loop
      integration, no async runtime). Added `i18n.rs` (ES/EN) + `widgets.rs`.
- [x] **Account** (~109 LOC) — email + storage bar via `rclone about`/`backend
      user`, Open Drive/Manage actions. Blocking rclone runs on a background
      thread; results marshaled back through `mpsc` + `glib::idle_add_local`.
- [x] **Activity** (~78 LOC) — activity log viewer (last 100, newest first),
      search filter + clear.
- [x] **Selective** (~70 LOC) — remote folder listing (`rclone lsd`) with
      per-folder switches persisted to `settings.json`.
- [ ] **Synchronization** (~194), **Settings** (~317).

### Phase 3 — Tray in Rust

- [ ] Port `gdrive-tray.py` (~480 LOC) → `tray-icon` crate
      (StatusNotifierItem / KStatusNotifier).
- [ ] Update `gdrive-tray.service` (systemd user unit) to launch the Rust binary.

### Phase 4 — Integration, i18n, packaging

- [ ] Port `i18n.py` (ES/EN) → Rust i18n (rust-i18n or fluent); reuse existing
      keys.
- [ ] Update packaging: `.deb`, `install.sh`, `Makefile` to ship Rust binaries.

### Phase 5 — Final cutover and legacy removal

- [ ] Remove Python UI: `gdrive/*.py` (UI-only), `gdrive-app.py`,
      `gdrive-tray.py`, `launch.sh`.
- [ ] Keep `native/gdrive-core` untouched; update `AGENTS.md` and
      `CONTRIBUTING.md` to reflect the new architecture.

## 6. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Large rewrite (6 pages) | Schedule | Phase 2 splits by page; Status page proves the pattern first |
| Tray GTK3 constraint | Regressions | Migrate tray independently in Phase 3; keep Python tray live during transition |
| D-Bus contract drift | Daemon/tray breakage | Freeze spec in Phase 0; add integration tests against the daemon |
| i18n drift | Lost translations | Reuse the exact keys from `i18n.py` |
| Packaging regressions | Broken install | Phase 4 updates `.deb`/scripts with the new binaries |

## 7. Open Decisions

Resolved on 2026-09-06:

1. **UI↔core integration**: **D-Bus contract** (lower risk). The Rust UI keeps
   talking to the daemon over the existing `com.gdrive.Daemon1` interface; the
   daemon and Python tray remain untouched during the transition.
2. **Tray during transition**: keep the Python/GTK3 tray live while the window
   migrates (migrated independently in Phase 3).
3. **i18n scope**: keep **ES + EN** (the app has an English option; both are
   preserved).

### System prerequisite (blocking first compile)

`gtk4-rs` needs the development headers, currently missing on this machine:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

Runtime libraries are already present (`libgtk-4-1` 4.22.4, `libadwaita-1-0`
1.9.1); only the `-dev` packages (which ship the `.pc` pkg-config files) are
required to build the Rust bindings.

## 8. Suggested First Step

Start **Phase 0 + Phase 1**: create the `feature/rust-ui` branch and the
`gdrive-ui` crate, port the window skeleton + Status page, connected to the
daemon over the existing D-Bus contract (lowest risk). Keep the Python tray
running during the transition.