# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Developer-facing text is now English throughout.** Every code comment,
  docstring, module doc, packaging file and installer message was translated.
  The application's own strings did not change: the Spanish and English
  catalogs are byte-identical, so a Spanish locale still sees exactly the same
  UI as before.
- `.desktop` entries now ship an English `Comment=` plus a `Comment[es]`
  variant, so the desktop shell localizes them instead of always showing
  Spanish.

### Fixed

- The About page reported **1.0.0** while the daemon, the `.deb` and the release
  were 1.1.0. `APP_VERSION` is 1.1.0 now, and `Makefile` and `build-deb.sh` read
  the version from `DEBIAN/control` instead of hardcoding one — `build-deb.sh`
  was about to name the artifact `…-1.0.1.deb` against a `1.1.0` control file.
- `desktop-files/google-drive.desktop`, and the copy `install.sh` generates,
  left an unquoted `?` in `Exec=`; `desktop-file-validate` rejects that, so the
  URL is now quoted.
- `DEBIAN/control` described the package as a Google Drive client *for macOS*.
  It now describes the Ubuntu/Linux client built on rclone.
- The core docs and module headers still referenced `gdrive/sync.py`, which was
  removed in 1.1.0; they now say it was removed.

### Known defect

- `desktop-files/gdrive-settings.desktop` ships `Exec=bash RUTA_DEL_PROYECTO/launch.sh`
  and nothing substitutes that placeholder, so that launcher cannot work from
  the `.deb`. The file now documents this instead of claiming `setup.sh` fills
  it in; repairing it means deciding where the package installs the app.

## [1.1.0] - 2026-10-30

### Fixed

- **Local changes were never uploaded; Drive always won.** The sync engine
  derived the bisync listing prefix as `gdrive__…` while rclone names those
  files `gdrive_..`. The check never matched, so **every** run passed
  `--resync`, and `--resync` means `--resync-mode path1` — Path1 is the remote,
  so Drive's copies replaced newer local files on every sync. The prefix is now
  derived the way rclone does, and a resync is only used to seed a missing
  baseline, always with `--resync-mode newer`.
- **The bisync baseline was destroyed on failed runs.** A helper promoted
  `*.lst-new` over `*.lst` even when the run had aborted, leaving header-only
  listings (60 bytes against the 8.7 MB intact `-err` file) and forcing a
  resync forever. rclone already performs that rename on success, so the helper
  was removed.
- **Sync progress never reached the UI.** Without `-v` rclone emits no output at
  all (verified: zero bytes written while copying 1 GiB), so the progress bar
  never moved. With `-v` the real format is
  `512 MiB / 2 GiB, 25%, 12.3 MiB/s, ETA 10s` — decimals and different units on
  each side — which the old parser could not read. The engine now passes `-v`
  and normalizes sizes to bytes.
- **rclone could hang mid-sync.** The engine read `stdout` (always empty) while
  leaving `stderr` unread; once the 64 KB pipe filled on large trees the process
  blocked, and the wall-clock timeout — evaluated only when a line arrived —
  never fired. The engine now reads `stderr`, and the guard is an inactivity
  timeout instead of a 10-minute wall that a legitimate run of ~54k files
  (15–20 min) could never fit under.
- **`--bwlimit` was emitted up to three times, using a syntax rclone rejects.**
  `--bwlimit d:500k` / `u:1M` return `invalid argument` in v1.74, and a repeated
  flag is silently overridden by the last one (measured: `1M 2M` transferred at
  2 MB/s). Setting a download or upload cap therefore broke every sync. One flag
  is emitted now, carrying the strictest configured value.
- **The sidebar status badge rendered as a vertical pill.** `.status-dot` asked
  for 8×8 and a 9999 px radius, but the box had no vertical alignment, so it
  stretched to the row's 36 px minimum height. It is a dot now. `set_badge_color`
  also created a fresh `Gtk.CssProvider` on every status change; the button keeps
  one and reloads it.
- **Orphan locks were never cleaned.** Cleanup looked for a lock path built from
  the wrong prefix and matched processes with `pgrep -f`. The PID recorded in the
  lock's own JSON is used now, and an unreadable lock is left in place rather
  than deleted blindly.
- **A second daemon could run unnoticed.** Requesting the D-Bus name queued the
  request, so a duplicate stayed alive without serving the bus but with its
  periodic scheduler active — two engines able to run bisync on the same pair.
  The name is requested with `DoNotQueue` and the process exits if it loses.
- **`launch.sh` could skip launching the app.** Its `pgrep -f` checks matched
  the command line of the shell invoking the script, so the launcher believed an
  instance was already running. Instance detection now uses D-Bus name ownership.
- **The tray's "Settings" opened nothing.** It launched a second UI process,
  which hit the single-instance guard, activated the existing window, showed the
  "already running" notice and exited. It now asks the running instance for the
  `go-settings` action.
- **Dialogs closed themselves before they could be read.** An
  `Adw.MessageDialog` with no transient parent is not registered as an
  application window, so `run()` returned as soon as `activate` ended; and
  confirm dialogs kept their only reference in a local variable, so they were
  finalized while on screen (measured with a weak reference). Both now hold a
  reference for their lifetime and release it on destroy.

### Changed

- `conflict_action` is now actually consumed: valid rclone resolutions are
  passed to `--conflict-resolve`, anything else falls back to `newer`.
- Conflicts and interrupted runs no longer demand a full resync
  (`--recover`, `--resilient`, `--conflict-resolve newer`).
- `--fast-list` is passed to rclone: measured on Google Drive, listing a
  sub-tree went from 18.74 s to 1.12 s, which also cuts the API calls behind the
  `rateLimitExceeded` aborts seen in logs.
- Startup no longer opens a window: the app lives in the tray until asked
  (`start_hidden`, toggle in Settings).
- Settings knobs that no code read were removed or renamed to match their
  effect: `cache_size_mb` (dropped), mode `stream` (dropped, not implemented),
  `delete_local` → `remove_empty_dirs`, `preserve_timestamps` → `store_metadata`.
- Default `excluded_patterns` now cover `node_modules`, `venv`, `.venv`,
  `__pycache__` and `target` at any depth (validated with `rclone lsf`: the
  `**/` prefix is required or nested directories are not excluded).

### Removed

- `gdrive/sync.py`, the legacy Python engine: unreachable from the UI and
  carrying the same always-resync bug, with its listing prefix hardcoded to one
  machine's disk path.

## [1.0.0] - 2026-09-06

Initial release: GTK4/libadwaita window with six pages, GTK3/AppIndicator3
system-tray daemon, `rclone bisync` synchronization, selective sync, bandwidth
and schedule controls, activity log with rotation and export, Nautilus
extension, and a native Rust core exposed over D-Bus.
