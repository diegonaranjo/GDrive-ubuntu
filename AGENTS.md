# AGENTS.md — Technical Guide for Contributors and AI Agents

Reference for anyone working on this codebase: architecture, conventions,
and operational notes. See also [CONTRIBUTING.md](CONTRIBUTING.md) for the
development workflow and contribution process.

---

## Project

**GDrive** — a Google Drive desktop client for Ubuntu. GTK4/libadwaita window +
GTK3 system-tray daemon + `rclone` synchronization, driven by a native Rust core.

- **App ID**: `com.gdrive.Client` (daemon) / `com.gdrive.ClientUI` (window)
- **Config**: `~/.config/gdrive/settings.json`
- **Sync target** (default): `~/GoogleDrive`

---

## Architecture

```
gdrive-app.py          # GTK4/libadwaita — main window (6 pages)
gdrive-tray.py         # GTK3/AppIndicator3 — system tray icon
launch.sh              # launches daemon + tray + window
install.sh             # system install (requires root)
build-deb.sh           # builds the .deb package
verify.sh              # verifies the installation
Makefile               # build / install / verify / uninstall targets
native/gdrive-core/    # native Rust core: config, sync (rclone bisync), logging, i18n, D-Bus daemon
native/gdrive-ui/      # Rust port of the GTK4 window (WIP: compiles, not installed, not in CI)
DEBIAN/control         # .deb metadata
nautilus-extension/    # Python Nautilus extension (context menu)
icons/                 # google-drive.svg + Lucide theme
desktop-files/         # .desktop launchers
```

### Architecture decisions

- **Separate processes**: `AyatanaAppIndicator3` is GTK3-only and cannot coexist
  with GTK4 in the same process.
- **Tray via systemd**: `gdrive-tray.service` (user service).
- **Defaults**: `DEFAULTS` dict in `gdrive/config.py` (~30 options).
- **Logging**: `log_write()` with rotation, levels (error/info/debug/trace),
  export to zip.
- **Native Rust core**: the `gdrive-daemon` binary performs synchronization and
  is exposed over D-Bus (`com.gdrive.Client`, path `/com/gdrive/Client`). The
  GTK4 UI and the tray talk to it via `gdrive/daemon_client.py`.

---

## Rules

### GTK3 vs GTK4

`gdrive-app.py` uses GTK4/libadwaita; `gdrive-tray.py` uses GTK3/AppIndicator3.
**Never mix GTK versions in the same process.**

GTK4 idioms:

- `append()`, `set_margin_end()`, `set_hexpand()`

GTK3 idioms:

- `pack_start()`, `pack_end()`, `set_margin_right()`, `set_margin_left()`

### Rust (`native/gdrive-core/`)

Apply the [Apollo GraphQL Rust Best
Practices](https://github.com/apollographql/rust-best-practices):

1. **Typed errors** with `thiserror` (`src/error.rs`); never `Result<T, String>`.
2. **No `unwrap`/`expect` in production code.** `Mutex::lock().unwrap()` is
   tolerated for poisoning recovery (documented rationale).
3. **Document every public item** (`#![deny(missing_docs)]`; the `daemon` module
   allows `missing_docs` because zbus's `#[interface]` macro generates
   undocumented public glue).
4. **Import order** `std` → external → crate (enforced manually; rustfmt's
   unstable import grouping is intentionally not used).
5. **Lints** in `Cargo.toml` `[lints]`; CI runs
   `cargo clippy --all-targets --all-features --locked -- -D warnings`.

---

## Icons

We use **Lucide** symbolic SVG icons, installed to
`~/.local/share/icons/hicolor/scalable/`.

### Sidebar icons

| Page           | Icon               |
|----------------|--------------------|
| Status         | `info-symbolic`    |
| Account        | `user-symbolic`    |
| Synchronization| `refresh-symbolic` |
| Folders        | `folder-symbolic`  |
| Settings       | `settings-symbolic`|
| Activity       | `clock-pause-symbolic` |

### Status icons (main page)

Custom SVGs using Google Drive colors:

| State      | Icon                            |
|------------|---------------------------------|
| Idle/Ready | `gdrive-status-symbolic` (green + check) |
| Syncing    | `gdrive-syncing-symbolic` (blue + arrows) |
| Paused     | `gdrive-paused-symbolic` (yellow + pause) |
| Error      | `gdrive-error-symbolic` (red + X) |

### Action icons

| Use           | Icon                   |
|---------------|------------------------|
| Log files     | `file-text-symbolic`   |
| Open folder   | `folder-open-symbolic` |
| Web browser   | `globe-symbolic`       |
| Documentation | `help-circle-symbolic` |
| Errors        | `alert-circle-symbolic`|

### Adding an icon

1. Verify it exists:
   `python3 -c "import gi; gi.require_version('Gtk','4.0'); from gi.repository import Gtk,Gdk; t=Gtk.IconTheme.get_for_display(Gdk.Display.get_default()); print(t.has_icon('<name>-symbolic'))"`
2. If missing, download from
   `https://raw.githubusercontent.com/lucide-icons/lucide/main/icons/<NAME>.svg`
3. Copy to `~/.local/share/icons/hicolor/scalable/<CONTEXT>/`
4. Run `gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor`
5. **Do not use the `ui/` folder** — GTK4 does not search there for symbolic
   icons.

---

## App pages (`gdrive-app.py`)

1. **Status** (`StatusPage`) — sync status, info, sync/pause buttons
2. **Account** (`AccountPage`) — email, storage bar (rclone `about`)
3. **Synchronization** (`SyncSettingsPage`) — folder, mode, interval, bandwidth, threads
4. **Folders** (`SelectiveSyncPage`) — selective sync via rclone folders
5. **Settings** (`SettingsPage`) — general, network, cache, encryption, logs, about
6. **Activity** (`ActivityPage`) — activity log

### Sidebar

- `Gtk.Paned` with a `Gtk.Box` sidebar (170px, `hexpand=False`) + separator + content
- `SidebarButton` widgets with the `.sidebar-item` CSS class
- Direct navigation via `content_box` (remove the old child, add the new one)

---

## Configuration (`~/.config/gdrive/settings.json`)

~30 options; see `DEFAULTS` in `gdrive/config.py`. Key options:

- `sync_dir`: destination folder (default `~/GoogleDrive`)
- `sync_interval`: `manual`, `5min`, `15min`, `30min`, `1hr`
- `sync_mode`: `mirror`, `stream`, `selective`
- `paused`: boolean
- `bandwidth_limit`: empty string = no limit
- `logging_enabled`, `log_level`, `log_rotate`, `log_max_size_mb`, `log_max_files`

---

## systemd services

```bash
# Tray icon
systemctl --user status gdrive-tray.service
systemctl --user restart gdrive-tray.service
systemctl --user enable gdrive-tray.service
```

The service runs `gdrive-tray.py` as the tray daemon.

---

## Known issues

- **Nautilus crash from dual-loading libpython3.14**: having a custom Python in
  `/usr/local/lib/` together with the system one in `/usr/lib/` causes
  `free(): invalid pointer`. Fix: move the custom libs.
- **GNOME Online Accounts has no Google Drive**: on Ubuntu 24.04+, GOA 3.58
  dropped file support. Use rclone.
- **`rclone authorize` does not auto-update the config**: the token must be
  copied manually into `~/.config/rclone/rclone.conf`.

---

## Useful commands

```bash
# Run the app
python3 gdrive-app.py

# Restart the tray
systemctl --user restart gdrive-tray.service

# Watch logs
tail -f ~/.config/gdrive/debug.log

# Verify the installation
./verify.sh

# Build the .deb
make build

# Manual sync
rclone bisync gdrive:/ ~/GoogleDrive --resync
```