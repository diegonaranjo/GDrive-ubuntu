# Security Policy

GDrive is a desktop client: it stores an rclone OAuth token for your Google
account and moves your files through it. Treat that token as a credential.

## Supported versions

| Version | Supported |
|---------|-----------|
| latest  | yes       |
| older   | best effort, upgrades welcome |

## What this project considers a security issue

- Anything that could leak the rclone token, your account email, or synced
  file contents to a third party.
- Command construction that allows argument or shell injection into `rclone`.
- Path traversal that lets a remote file write outside the sync directory.
- Sync-engine behavior capable of silently destroying user data (a bad
  baseline, a forced `--resync` where the remote wins, an unbounded delete).

## What is not a vulnerability

- Requiring the user's own credentials to run the app.
- The fact that `rclone` is a separate binary the user installs.
- Missing encryption: GDrive stores no secrets of its own; at-rest encryption
  is delegated to Google Drive and to your disk.

## Reporting

Please do **not** open a public issue. Email
`diegonaranjo@users.noreply.github.com` with the word "security" in the
subject. Include the version (`gdrive-app.py --version` or `APP_VERSION` in
`native/gdrive-core/src/constants.rs`), your distribution, and reproduction
steps if you have them.

We aim to acknowledge reports within 5 business days. We do not currently offer
a paid bounty program.
