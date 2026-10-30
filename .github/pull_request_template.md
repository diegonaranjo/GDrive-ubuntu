## What does this change?

Describe the behavior change, not just the diff. Link the issue if there is one.

## Why

What problem does it solve? If this fixes a bug, include how you reproduced it.

## How it was verified

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo test`
- [ ] Python `py_compile` over `gdrive/` and the two entry points
- [ ] Behavior checked by running the app or the daemon (describe below)

## Data-safety note

If the change touches `native/gdrive-core/src/sync.rs`, state what happens to
existing bisync listings and whether a run could delete data on either side.

## Screenshots

For UI changes, attach before/after.
