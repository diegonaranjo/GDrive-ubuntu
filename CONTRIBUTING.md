# Contributing to GDrive

Thank you for your interest in contributing! GDrive is a Google Drive desktop
client for Ubuntu built with a GTK4/libadwaita Python UI and a native Rust
core. This guide helps you set up a development environment and explains the
conventions we follow.

## Code of Conduct

Be kind, be respectful, and assume good intentions. Keep discussions focused
on the project. Harassment, discrimination, or unprofessional behavior is not
tolerated.

## Getting started

### Prerequisites

- Ubuntu 22.04 LTS+ (or any distro shipping GNOME and GTK 4.10+)
- Python 3.10+
- `rclone` ([install](https://rclone.org/install/))
- A stable Rust toolchain with `rustfmt` and `clippy` (see below)

### Clone and install

```bash
git clone https://github.com/diegonaranjo/GDrive-ubuntu.git ~/GDrive
cd ~/GDrive
./setup.sh          # user-scoped install (icons, .desktop, tray service, daemon)
```

`setup.sh` runs **without sudo** and only touches `~/.local`, `~/.config` and a
user-scoped systemd service.

### Rust toolchain

The native core lives in [`native/gdrive-core/`](native/gdrive-core/). It is
pinned to the `stable` channel with the `rustfmt` and `clippy` components via
[`rust-toolchain.toml`](native/gdrive-core/rust-toolchain.toml). Install it with:

```bash
rustup default stable
```

`rustup` will automatically select the pinned toolchain whenever you work
inside `native/gdrive-core/`.

## Project layout

```
gdrive-app.py            # GTK4/libadwaita main window (entry point)
gdrive-tray.py           # GTK3/AppIndicator3 system-tray daemon
gdrive/                  # Python package (pages, config, D-Bus client, helpers)
native/gdrive-core/      # Rust core: config, sync (rclone bisync), logging, i18n, D-Bus daemon
launch.sh                # launches daemon + tray + window
setup.sh                 # user-scoped install
build-deb.sh             # builds the .deb package
```

### Architecture at a glance

The heavy lifting (sync, config, logging, i18n) lives in the Rust core and runs
as a D-Bus daemon (`gdrive-daemon`). The Python UI and tray talk to it over
D-Bus through [`gdrive/daemon_client.py`](gdrive/daemon_client.py). `rclone` is
the transfer engine, orchestrated by the daemon.

> **GTK versions must never be mixed in the same process.** `gdrive-app.py` is
> GTK4/libadwaita; `gdrive-tray.py` is GTK3/AppIndicator3. Keep them as separate
> processes.

## Development workflow

### Rust core

```bash
cd native/gdrive-core

cargo fmt --all -- --check           # formatting
cargo clippy --all-targets --all-features --locked -- -D warnings   # linting (strict)
cargo test                           # tests
cargo build --release                # generates target/release/gdrive-daemon
```

All four steps must pass in green before opening a pull request. The CI runs
exactly these checks (plus a Python syntax check).

### Python UI

```bash
python3 -m py_compile gdrive/*.py gdrive/pages/*.py gdrive-app.py gdrive-tray.py
```

For local UI testing, launch with `bash launch.sh` (it starts the native
daemon, the tray and the window).

## Coding conventions

### Rust (Apollo GraphQL best practices)

We follow [Apollo's Rust Best Practices
Handbook](https://github.com/apollographql/rust-best-practices). In brief:

- **Typed errors** via `thiserror` (see `native/gdrive-core/src/error.rs`); no
  `Result<T, String>`.
- **No `unwrap`/`expect` in production code.** `Mutex::lock().unwrap()` is
  tolerated for posioning recovery (documented).
- **Document all public items** — the crate uses `#![deny(missing_docs)]`.
- **Import order** `std` → external → crate (enforced manually; `rustfmt`'s
  unstable import grouping is intentionally not used).
- **Lints** are configured in `Cargo.toml` under `[lints]` and checked by CI.

### Python

- GTK4 and GTK3 APIs are never mixed in one process (see above).
- Configuration reads/writes go through the `DEFAULTS` + `load_config`/
  `save_config` helpers in `gdrive/config.py`.

### Language

Dialogue with the maintainer is in Spanish, but **all committed documentation
(README, CONTRIBUTING, code comments, commit messages) is written in English.**

## Commit messages

Keep commits small, focused and atomic. Use a clear imperative subject line,
optionally with a scope:

```
fix(sync): derive bisync prefix from sync_dir instead of hardcoding it
tests(core): add run_with_callback progress contract
```

### Commit identity and the guard

A pushed commit is effectively permanent. Rewriting refs on GitHub does not
remove the objects: they keep answering over the API and the tarball endpoint
until the whole repository is deleted, which is the only "undo" and costs the
history. So the project refuses personal or client identities *before* a push.

`setup.sh` calls `tools/install-guard.sh`, which sets
`core.hooksPath = tools/githooks`, and `tools/secret-scan.sh` then runs on every
commit and every push. The installer also pins a noreply identity **for that
clone** when the clone would otherwise inherit a personal one (repo-local only —
it never edits your global config). It refuses:

- a signing identity listed in `.git/git-guard.deny` — untracked, seeded from
  your global `user.email` when that is not a noreply address;
- in the maintainer's clone (`gitguard.strict-identity=true`), any identity that
  is not `<name>@users.noreply.github.com`;
- secret shapes in the diff: private-key headers, `ghp_` / `github_pat_` /
  `AKIA` / `xox*` tokens, non-empty `access_token` / `refresh_token` /
  `client_secret`, `Authorization: Bearer|Basic`, and `user:pass@host` URLs;
- absolute paths that belong to the machine running the check (derived at run
  time from `$HOME` and `$USER`, so the rule never stores anyone's identity);
- file names that must not be tracked: `.env`, `*.pem`, `*.key`, `*.p12`,
  `id_rsa*`, `rclone.conf`, `credentials*`, `.netrc`, keyrings.

A test that needs a credential-shaped string builds it at run time (see
`tools/guard-battery.sh`): a literal fixture would be published with the
repository, and would also trip the scanner on its own test file. After changing
`tools/secret-scan.sh`, run `tools/guard-battery.sh` — 13 cases, each asserting
the exit code it expects, including the ones that must fail.

Give the clone an explicit identity (`./setup.sh` or `tools/install-guard.sh`
does this for you):

```bash
git config user.email "you@users.noreply.github.com"
```

and make every future clone of that path inherit it, which is the part that
actually prevents the mistake from coming back:

```ini
; ~/.gitconfig
[includeIf "gitdir:~/repos/GDrive/"]
  [user]
    email = you@users.noreply.github.com
```

Ordinary contributor identities are not refused, only noted; allow one
explicitly by adding it to `.git/git-guard.local`. `git push --no-verify`
skips the local hooks, but the CI guard still scans the pushed diff — and what
gets through is still yours to explain, because it cannot be taken back.

## Testing

- Rust: `cargo test` in `native/gdrive-core/`. Add a test for every bug fix and
  new public API. Name tests descriptively (e.g.
  `run_with_callback_receives_progress`).
- Python: at minimum, run `py_compile` as shown above.

## Continuous Integration

GitHub Actions (`.github/workflows/ci.yml`) runs on every push and pull
request:

- Identity & secret guard: `tools/secret-scan.sh range` over the pushed commits.
- Rust: `cargo fmt --check`, `cargo clippy --all-targets --all-features
  --locked -- -D warnings`, `cargo test`, `cargo build --release`.
- Python: syntax compilation of the UI package.

## Reporting issues

When opening an issue, include:

1. OS and desktop environment (e.g. Ubuntu 24.04, GNOME 46).
2. Steps to reproduce.
3. Expected vs. actual behavior.
4. Any relevant log output from `~/.config/gdrive/debug.log`.

## Pull request checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features --locked -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] Python `py_compile` passes
- [ ] Public API is documented (`#![deny(missing_docs)]`)
- [ ] New behavior is covered by tests
- [ ] Documentation is in English