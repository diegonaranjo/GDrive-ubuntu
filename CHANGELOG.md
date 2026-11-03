# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `tools/githooks/commit-msg` refuses a commit whose subject already exists in
  the last 60 commits: a heredoc message pasted from an earlier commit silently
  describes the wrong change, and it happened twice in one session. A
  message-only amend stays allowed, detected by comparing the staged tree with
  HEAD's tree.
- **`tools/secret-scan.sh`**, an identity and secret guard that runs on every
  commit (`tools/githooks/pre-commit`), every push (`pre-push`) and in CI (the
  new `guard` job). Pushed commits are permanent — after a ref rewrite GitHub
  still serves the old objects by SHA on the commit API and the tarball
  endpoint — so the check belongs before the push, not after it. It refuses: an
  address listed in the untracked `.git/git-guard.deny` (seeded by `setup.sh`
  from the machine's global `user.email` when that is not a noreply address),
  any non-noreply signing identity when the clone sets
  `gitguard.strict-identity=true`, secret shapes in the diff (private keys,
  `ghp_`/`github_pat_`/`AKIA`/`xox*`, non-empty `access_token`/`refresh_token`/
  `client_secret`, `Authorization:` headers, `user:pass@host` URLs), absolute
  paths belonging to the machine running it, and file names such as `.env`,
  `*.pem`, `rclone.conf` or `credentials*`.
- **`tools/install-guard.sh`**, called by `setup.sh`, installs those hooks and
  closes the hole that caused the original leak: a *fresh* clone inherited the
  machine's global identity. The installer pins a noreply `user.email` for that
  clone (repo-local only; the global config is left alone), seeds
  `.git/git-guard.deny` with the global address so it can never sign a commit
  here, and turns on `gitguard.strict-identity`. Running it twice is a no-op,
  and the four scripts are committed with mode 755 so a clone gets working
  hooks. Verified on a throwaway clone: it started with the business address
  inherited from the global config and ended signing
  `<login>@users.noreply.github.com`, then refused a commit that staged a
  `/media/<user>/...` path and accepted an innocent one.
- The policy is deliberately asymmetric: an ordinary contributor identity gets a
  note, not a refusal, so the guard cannot be used to bully a pull request. The
  maintainer's clone is the one that gets strict signing, and the rule is
  enforced where it belongs — `~/.gitconfig` `includeIf` per path — so a *new*
  clone cannot reintroduce the mistake.
- **`tools/guard-battery.sh`**: 13 cases over the scanner, each declaring the
  exit code it must produce, including the ones that must fail, plus a check
  that a finding is reported once rather than per duplicated pattern. Run it
  after any change to `tools/secret-scan.sh`. It caught two regressions while
  this was being written — a subshell that made the script report findings
  while exiting 0, and a deduplication that silently dropped `/media/<user>`
  from the path list.
- The guard stores no personal identifier: the path rules are derived at run
  time from `$HOME`/`$USER` and the identity list lives in untracked files, so
  the tool meant to stop a leak cannot itself become one.

### Fixed

- **Every Rust rclone wrapper was broken, and leaked output.** `run()` left the
  child's stdout inherited and then called `wait_with_output()` after
  `try_wait()` had already reaped the process; the second wait fails with
  ECHILD, so each wrapper returned an error. `gdrive-core authorized` answered
  `false` while the real `rclone about gdrive:` returned valid JSON, and
  `gdrive-core email` dumped the raw remote config to the terminal. Both
  streams are piped now, stdout is drained on a thread (a full 64 KB pipe
  would block the child before it could exit), and a non-zero exit carries
  rclone's stderr in the error. The CLI now reports `authorized → true` and
  parses the storage JSON. Two new tests cover the wrappers — they had none,
  which is how this survived — and both fail when the buggy body is put back.
- **`activity.log` never rotated.** It had grown to 46 MB on a real install
  because the rotation policy (`log_rotate` / `log_max_size_mb` /
  `log_max_files`) was applied only to `debug.log`. Both logs now share one
  rotation implementation in each language, and the shift/prune chain is
  exercised by a test that uses a byte cap instead of a megabyte.
- **One activity entry could be enormous.** A legacy sync run wrote a single
  23 MB multi-line entry (the whole rclone stderr, twice). An activity entry is
  now flattened to one line and capped at 400 characters, with a pointer to
  `debug.log`, which still receives the full text.
- **The Activity page parsed the whole log to paint 100 rows.** It now reads the
  last 128 KB and drops the partial first line. Measured on the real 47.8 MB
  file: 141 ms and 8,247 lines read, versus under a millisecond and the 5
  requested lines.
- `gdrive/config.py` hardcoded `~/.config/gdrive` while the Rust core honored
  `GDRIVE_CONFIG_DIR`; the two now use the same rule, so the UI can be pointed
  at an isolated directory like the daemon already could.
- The bug-report template asked for `journalctl --user -u gdrive-tray.service`
  only, and its example said "GDrive 1.0.0". It now names both user units and
  the current version, so the first thing a triager sees is the log of the unit
  that actually runs the sync.

## [1.2.0] - 2026-11-02

### Added

- **`gdrive-daemon.service`**, a supervised user service for the sync engine,
  generated by `setup.sh` next to the tray unit (`Restart=on-failure`,
  `RestartSec=5`, `PartOf=graphical-session.target`). Before this, the engine
  was an orphan of the autostart entry: nothing restarted it if it died, and
  its `rclone` children lived outside any unit. Verified on a live session: a
  `SIGKILL` of the engine is picked up by systemd (`NRestarts=1`, a new PID
  re-acquires `com.gdrive.Client`) while the UI and tray keep talking to the
  name. `launch.sh` and `gdrive/helpers.py` now start the engine through the
  unit when it exists, and fall back to spawning the binary when it does not.

### Changed

- **Developer-facing text is now English throughout.** Every code comment,
  docstring, module doc, packaging file and installer message was translated.
  The application's own strings did not change: the Spanish and English
  catalogs are byte-identical, so a Spanish locale still sees exactly the same
  UI as before.
- `.desktop` entries now ship an English `Comment=` plus a `Comment[es]`
  variant, so the desktop shell localizes them instead of always showing
  Spanish.
- The `gdrive-core` CLI (`about|authorized|email|version`) now prints its usage
  and failure lines in English, as does the one panic message in the daemon.
  These are terminal/panic text, never rendered in the window. Text that *does*
  reach the UI (`#[error(...)]` bodies, daemon log lines, the Nautilus menu) is
  still Spanish-only and is tracked in [#1](https://github.com/diegonaranjo/GDrive-ubuntu/issues/1).
- Version 1.2.0, with `DEBIAN/control` as the single source: `Makefile`,
  `build-deb.sh` and `show-structure.sh` read it, and `APP_VERSION`
  (Python), `gdrive-core` (Rust) and the installer banner match it, so the
  About page, `gdrive-core version`, `GetStatus` and the `.deb` all agree.

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
- **`debug.log` carried two clocks.** The Rust core derived its stamp from
  `SystemTime` alone, which is UTC, while the Python writer used
  `time.strftime` (local) and the daemon embeds rclone's own local timestamps
  in the very lines it logs. On a host whose offset is not UTC that put a tray click and the sync
  it triggered three hours apart, in adjacent lines of one file. Both loggers
  now use local time (`chrono::Local`), covered by a test that compares the
  stamp against `date` rather than against the library under test.
- A **second daemon exited with status 1** when another instance already owned
  `com.gdrive.Client`. That is the intended singleton outcome, so it now exits
  0 — otherwise systemd's `Restart=on-failure` would relaunch the duplicate
  forever beside the real owner.
- The tray unit `setup.sh` generated was **wanted by `default.target` and had no
  `PartOf=`**, unlike the unit actually installed on a working machine. Without
  `PartOf=` the tray is not stopped when the session target goes down: it
  outlives the session until the display connection dies under it, GTK exits 1,
  and `Restart=on-failure` relaunches a copy that dies again a few hundred
  milliseconds later — the `status=1/FAILURE` pair visible in the journal at
  every logout of the last three sessions. Both units now sit under
  `graphical-session.target` with `PartOf=`, and `AGENTS.md` documents how to
  verify it, including a scratch-target recipe for measuring `PartOf` without
  ending your own desktop (measured: the `PartOf` unit stops
  `inactive/dead Result=success NRestarts=0`; the one without it keeps running).
- The README and `AGENTS.md` told users to run `rclone bisync … --resync` as the
  manual-sync recipe. Plain `--resync` is `--resync-mode path1`, i.e. **the
  remote wins**, which is the exact data loss this release fixed. The recipes
  now use `--conflict-resolve newer --resilient`, with `--resync` reserved for
  building a first baseline and always paired with `--resync-mode newer`.

### Known defect

- `desktop-files/gdrive-settings.desktop` ships `Exec=bash RUTA_DEL_PROYECTO/launch.sh`
  and nothing substitutes that placeholder, so that launcher cannot work from
  the `.deb`. The file now documents this instead of claiming `setup.sh` fills
  it in; repairing it means deciding where the package installs the app.
  Tracked with the rest of the packaging gap in
  [#2](https://github.com/diegonaranjo/GDrive-ubuntu/issues/2) — no `.deb` is
  published in any release, so this is latent rather than shipped.

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
