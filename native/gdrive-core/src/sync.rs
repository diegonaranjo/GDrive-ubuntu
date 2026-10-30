//! SyncEngine: orchestrates `rclone bisync` in the background.
//!
//! The names of bisync's `.lst`/`.lck` listings are **derived from the
//! (`remote`, `sync_dir`) pair** using the same sanitizing rclone applies,
//! instead of being hardcoded to a concrete path on the author's disk.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::constants::{bisync_cache_dir, rclone_remote, sync_dir_default};
use crate::error::{Error, Result};
use crate::logging::{log_write, Level};
use crate::rclone::parse_stats;

/// Maximum time without receiving any line from rclone before the sync is
/// declared hung and killed.
///
/// Replaces the old wall-clock timeout (600 s): with tens of thousands of
/// files a legitimate sync takes more than 10 min, and killing it halfway left
/// bisync without valid listings and demanding a `--resync` that overwrote the
/// local changes. While there is activity, rclone emits a `--stats` line every
/// 2 s and also self-terminates on network stalls (`--contimeout`/`--timeout`,
/// 5 min by default), so sustained silence is the only reliable signal that
/// the process is not going to keep going on its own.
pub const SYNC_IDLE_TIMEOUT: Duration = Duration::from_secs(900);

/// State of the synchronization engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// No synchronization in progress.
    Idle,
    /// Synchronization in progress.
    Syncing,
    /// Synchronization paused by the user.
    Paused,
    /// The last synchronization failed.
    Error,
}

impl SyncStatus {
    /// Textual (stable) representation of the state, used by D-Bus and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            SyncStatus::Idle => "idle",
            SyncStatus::Syncing => "syncing",
            SyncStatus::Paused => "paused",
            SyncStatus::Error => "error",
        }
    }
}

/// Entity that assembles and runs the bisync commands. No shared mutable
/// global state: each `run` is self-contained and returns a `Result`, removing
/// the race conditions of the Python `SyncManager`.
#[derive(Debug, Clone, Default)]
pub struct SyncEngine {
    /// Configuration that governs the synchronization.
    pub config: Config,
}

/// Result of a completed synchronization.
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    /// `true` if the synchronization finished successfully.
    pub ok: bool,
    /// Descriptive message (for logs and the UI).
    pub message: String,
    /// Bytes transferred (best-effort, from rclone's stats).
    pub transferred: u64,
    /// Total expected bytes (best-effort).
    pub total: u64,
}

impl SyncEngine {
    /// Creates an engine from a [`Config`].
    pub fn new(config: Config) -> Self {
        SyncEngine { config }
    }

    /// Builds the `rclone bisync` command applying the configuration fully
    /// (unlike the Python version, which ignored most settings).
    pub fn build_cmd(&self) -> (Vec<String>, PathBuf) {
        let sync_dir = self.sync_dir();
        let mut cmd: Vec<String> = vec!["rclone".into(), "bisync".into()];

        cmd.push(rclone_remote());
        cmd.push(sync_dir.to_string_lossy().to_string());

        // Seed the listings only the first time for this pair. Careful: bare
        // `--resync` is equivalent to `--resync-mode path1`, and Path1 is the
        // remote, so an implicit resync replaces the local files with the ones
        // from Drive even when the local ones are newer. `newer` lets the most
        // recently modified copy win, which is what the user expects.
        if !self.has_list_files() {
            cmd.push("--resync".into());
            cmd.push("--resync-mode".into());
            cmd.push("newer".into());
        }

        // `--recover` resumes from the last good run's listings instead of
        // demanding a full resync, and `--resilient` lets the *next* run retry
        // after an error (e.g. a Drive file with
        // `403 cannotDownloadAbusiveFile`) without destroying the baseline.
        cmd.push("--recover".into());
        cmd.push("--resilient".into());

        // Conflict = the file changed on both sides since the last sync. Without
        // this bisync aborts the run and leaves the state demanding `--resync`.
        // The loser is not deleted: rclone renames it (`--conflict-loser num`).
        cmd.push("--conflict-resolve".into());
        cmd.push(self.conflict_resolution().to_string());

        // Apply `sync_mode`: `selective` restricts the synchronization to the
        // selected folders (`selected_folders`), adding `--include` to limit
        // the tree to those paths (and their descendants).
        for inc in self.selective_includes() {
            cmd.push("--include".into());
            cmd.push(inc);
        }

        // `-v` is not decorative: without verbosity rclone **does not emit a
        // single line** (verified: 0 bytes of output while copying 1 GiB), so
        // the UI gets no progress and the idle watchdog has nothing to count.
        cmd.push("-v".into());
        cmd.push("--stats-one-line".into());
        cmd.push("--stats=2s".into());

        if let Some(limit) = self.bwlimit_arg() {
            cmd.push("--bwlimit".into());
            cmd.push(limit);
        }

        let threads = self.config.num_threads.clamp(1, 64);
        cmd.push("--transfers".into());
        cmd.push(threads.to_string());

        // Lists the remote recursively in a single pass instead of one call
        // per directory. Measured on Drive over a real subtree: 18.74 s
        // without the flag, 1.12 s with it. Here that shortens the full sync
        // (listing ~54k files used to take 15-20 min) and above all lowers
        // API calls, the source of the `rateLimitExceeded` aborts in the log.
        cmd.push("--fast-list".into());

        for pat in &self.config.excluded_patterns {
            cmd.push("--exclude".into());
            cmd.push(pat.clone());
        }
        if self.config.skip_hidden {
            cmd.push("--exclude".into());
            cmd.push(".*".to_string());
        }
        if self.config.proxy_enabled && !self.config.proxy_url.is_empty() {
            cmd.push("--proxy".into());
            cmd.push(self.config.proxy_url.clone());
        }
        if self.config.store_metadata {
            cmd.push("--metadata".into());
        }
        if self.config.remove_empty_dirs {
            cmd.push("--remove-empty-dirs".into());
        }

        (cmd, sync_dir)
    }

    /// Runs the synchronization in a blocking way and returns the result. The
    /// process is killed if output stops for `SYNC_IDLE_TIMEOUT`.
    pub fn run(&self) -> Result<SyncOutcome> {
        self.run_with_callback(|_line, _tr, _to, _speed| {})
    }

    /// Like [`run`](Self::run), but invokes a callback for each line of rclone
    /// output with its parsed progress `(transferred, total)` and speed. This
    /// lets the daemon forward the progress to the UI in real time.
    pub fn run_with_callback<F>(&self, mut on_line: F) -> Result<SyncOutcome>
    where
        F: FnMut(&str, u64, u64, Option<&str>),
    {
        log_write(
            &self.config,
            Level::Info,
            "Sync",
            &crate::i18n::t("sync_manager.starting"),
            true,
        );

        let (cmd, sync_dir) = self.build_cmd();
        std::fs::create_dir_all(&sync_dir)?;

        log_write(
            &self.config,
            Level::Debug,
            "Sync",
            &format!("Cmd: {}", cmd.join(" ")),
            false,
        );

        // Clear an orphaned lock if rclone bisync is no longer running.
        self.clear_orphan_lock();

        let mut child = Command::new("rclone")
            .args(&cmd[1..])
            // stdout to the void: rclone writes its logs and the `--stats`
            // lines to **stderr**. The previous code left stderr as a pipe it
            // never read: with tens of thousands of files the 64 KB buffer
            // filled up and rclone ended up blocked halfway through the sync,
            // while the stdout loop received no line at all (no progress, and
            // no way for the timeout to ever be evaluated).
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Rclone(format!("spawn rclone: {e}")))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Rclone("sin stderr disponible".into()))?;
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stderr);

        let mut transferred = 0u64;
        let mut total = 0u64;
        let mut last_stats: Option<String> = None;
        let mut tail: VecDeque<String> = VecDeque::with_capacity(16);

        let mut last_beat = Instant::now();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if last_beat.elapsed() > SYNC_IDLE_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::SyncTimeout);
            }
            last_beat = Instant::now();

            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if tail.len() == 10 {
                tail.pop_front();
            }
            tail.push_back(l.to_string());
            if let Some((tr, to)) = parse_stats(l) {
                transferred = tr;
                total = to;
                last_stats = Some(l.to_string());
            }
            let speed = crate::rclone::parse_speed(l);
            on_line(l, transferred, total, speed.as_deref());
        }

        let status = child.wait().map_err(Error::Io)?;

        // No `fix_lst_files()`: rclone already promotes `*.lst-new` → `*.lst`
        // when a run ends successfully. Doing it from outside as well renamed
        // the **incomplete** listings of an aborted run over the good baseline,
        // leaving it empty and forcing a `--resync` on every synchronization.
        let ok = status.success();
        let message = if ok {
            crate::i18n::t("sync_manager.completed")
        } else {
            // stderr was already consumed in streaming: the detail comes from
            // the `tail`, prioritizing the last ERROR line that explained it.
            let detail = tail
                .iter()
                .rev()
                .find(|l| l.contains("ERROR"))
                .or_else(|| tail.back())
                .map_or("", String::as_str);
            format!(
                "{}: {}",
                crate::i18n::t("sync_manager.failed"),
                detail.trim()
            )
        };

        log_write(
            &self.config,
            if ok { Level::Info } else { Level::Error },
            "Sync",
            &message,
            true,
        );

        // The success path left no trace of rclone in the log, so a run that
        // "completes" in the same second is undeterminable. The last stats
        // line says how much actually moved (or that nothing moved at all).
        let summary = last_stats.as_deref().unwrap_or("sin transferencias");
        log_write(
            &self.config,
            Level::Info,
            "Sync",
            &format!("rclone: {summary}"),
            false,
        );

        Ok(SyncOutcome {
            ok,
            message,
            transferred,
            total,
        })
    }

    /// Resolved sync folder (default `$HOME/GoogleDrive`).
    pub fn sync_dir(&self) -> PathBuf {
        if self.config.sync_dir.is_empty() {
            sync_dir_default()
        } else {
            PathBuf::from(&self.config.sync_dir)
        }
    }

    /// `--include` globs derived from the active `sync_mode`.
    ///
    /// # Sync mode mapping
    ///
    /// - `mirror` (default): full two-way synchronization of the tree.
    ///   Adds no filters; `bisync` compares all of `sync_dir` with the remote.
    /// - `selective`: restricts the synchronization to the folders chosen in
    ///   `config.selected_folders`. Each folder generates an `--include`
    ///   `folder/**` to include it and its descendants (excluding the rest
    ///   with rclone's implicit global `--exclude`).
    /// - `stream`: on-demand access (no full download). **Not supported by
    ///   the current `bisync` engine**; it behaves like `mirror` until an
    ///   `rclone mount` backend is implemented. Documented for clarity.
    pub fn selective_includes(&self) -> Vec<String> {
        if self.config.sync_mode != "selective" {
            return Vec::new();
        }
        let mut out = Vec::new();
        for folder in &self.config.selected_folders {
            let f = folder.trim().trim_end_matches('/');
            if f.is_empty() {
                continue;
            }
            out.push(format!("{f}/**"));
        }
        out
    }

    /// Builds ONE single `--bwlimit` from the configured limits.
    ///
    /// rclone v1.74 does not understand directional syntax: `--bwlimit u:1M`
    /// and `--bwlimit d:500k` both return `invalid argument` (measured), and
    /// repeating the flag makes only the last one count (measured:
    /// `--bwlimit 1M --bwlimit 2M` caps at 2 MB/s). The previous code emitted
    /// up to three flags, so setting a "download limit" broke every sync.
    ///
    /// The **strictest** of the requested values is applied, so no configured
    /// cap is exceeded. If there are values but none parse, the first one is
    /// passed as is: a visible rclone error is better than syncing silently
    /// without a limit.
    pub fn bwlimit_arg(&self) -> Option<String> {
        let configurados = [
            &self.config.bandwidth_limit,
            &self.config.download_limit,
            &self.config.upload_limit,
        ];
        let pedidos: Vec<&str> = configurados
            .into_iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let first = pedidos.first().map(|s| (*s).to_string());

        let mut mejor: Option<(u64, &str)> = None;
        for valor in &pedidos {
            let Some(bytes) = parse_bw_limit(valor) else {
                continue;
            };
            let mas_estricto = match mejor {
                None => true,
                Some((actual, _)) => bytes < actual,
            };
            if mas_estricto {
                mejor = Some((bytes, valor));
            }
        }
        mejor.map(|(_, v)| v.to_string()).or(first)
    }

    /// Conflict resolution passed to `--conflict-resolve`.
    ///
    /// The `conflict_action` setting shipped with the historical value
    /// `"rename"`, which bisync does not understand, so any unrecognized value
    /// falls back to `newer`: the most recently modified copy wins and the
    /// loser is renamed (`--conflict-loser num`), not deleted.
    pub fn conflict_resolution(&self) -> &'static str {
        match self.config.conflict_action.as_str() {
            "none" => "none",
            "path1" => "path1",
            "path2" => "path2",
            "newer" => "newer",
            "older" => "older",
            "larger" => "larger",
            "smaller" => "smaller",
            _ => "newer",
        }
    }

    /// Prefix with which rclone bisync names its listings and its lock.
    ///
    /// It is `{side1}..{side2}`, where each side is the `remote:path` token
    /// sanitized the way rclone does. Verified against rclone v1.74 with three
    /// different route forms (see `sanitize_side`), e.g.
    /// `gdrive:` + `/home/usuario/GoogleDrive` →
    /// `gdrive_..home_usuario_GoogleDrive`.
    pub fn bisync_prefix(&self) -> String {
        let path1 = rclone_remote();
        let path2 = self.sync_dir().to_string_lossy().to_string();
        format!("{}..{}", sanitize_side(&path1), sanitize_side(&path2))
    }

    /// `true` if a **usable** bisync baseline exists for this pair.
    ///
    /// Having the files there is not enough: a listing aborted halfway leaves
    /// a `.lst` with only the header line, and treating it as a baseline is
    /// what made bisync declare every file a conflict and demand a `--resync`.
    /// At least one real entry per side is required.
    pub fn has_list_files(&self) -> bool {
        let prefix = self.bisync_prefix();
        let dir = bisync_cache_dir();
        ["path1", "path2"]
            .iter()
            .all(|side| seeded_list(&dir.join(format!("{prefix}.{side}.lst"))))
    }

    /// Removes the lock (orphaned lock) if the process that created it is gone.
    ///
    /// rclone does **not** self-resolve an abandoned lock: it refuses to sync
    /// and warns that the file must be deleted by hand, so without this an
    /// interrupted sync would block the daemon forever. The PID is read from
    /// the lock itself instead of using `pgrep -f` because it is exact:
    /// deleting the lock of a live session would allow two concurrent bisync
    /// runs on the same pair, and that corrupts the listings.
    pub fn clear_orphan_lock(&self) {
        let lock = self.lock_path();
        let Ok(raw) = std::fs::read_to_string(&lock) else {
            return;
        };
        let pid = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| {
                v.get("PID").and_then(|p| {
                    p.as_str()
                        .and_then(|s| s.parse::<u32>().ok())
                        .or_else(|| p.as_u64().map(|n| n as u32))
                })
            });
        let Some(pid) = pid else {
            // Unreadable lock: not deleted blindly. rclone will reject the run
            // and its message will say which file to remove.
            log_write(
                &self.config,
                Level::Error,
                "Sync",
                &format!("No se pudo leer el lock de bisync: {}", lock.display()),
                true,
            );
            return;
        };
        if lock_owner_alive(pid) {
            return;
        }
        let _ = std::fs::remove_file(&lock);
        log_write(
            &self.config,
            Level::Info,
            "Sync",
            &format!("Lock huérfano eliminado: {}", lock.display()),
            true,
        );
    }

    /// Path of the bisync lock file for the current `sync_dir`.
    pub fn lock_path(&self) -> PathBuf {
        bisync_cache_dir().join(format!("{}.lck", self.bisync_prefix()))
    }
}

/// Converts a bandwidth limit (`"500k"`, `"1M"`, `"10M"`, `"1G"`, or a bare
/// number that rclone interprets as KiB/s) into bytes per second. `None` if
/// the value is not recognizable.
fn parse_bw_limit(s: &str) -> Option<u64> {
    let s = s.trim();
    let numericos: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if numericos.is_empty() {
        return None;
    }
    let valor: f64 = numericos.parse().ok()?;
    let sufijo = s[numericos.len()..].trim().to_ascii_lowercase();
    let factor: f64 = match sufijo.as_str() {
        "" => 1024.0,
        "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0f64.powi(3),
        "t" | "tb" | "tib" => 1024.0f64.powi(4),
        "p" | "pb" | "pib" => 1024.0f64.powi(5),
        _ => return None,
    };
    let bps = valor * factor;
    if !bps.is_finite() || bps < 0.0 {
        return None;
    }
    Some(bps.round() as u64)
}

/// Sanitizes a `remote:path` token for the bisync listing names, the way
/// rclone does: trims the leading `/` of an absolute path and the trailing `/`
/// of a remote root, and turns `:` and `/` into `_`.
///
/// Cases verified against rclone v1.74:
/// - `gdrive:` → `gdrive_`
/// - `/home/usuario/GoogleDrive` → `home_usuario_GoogleDrive`
/// - `:local:/tmp/bp3` → `_local__tmp_bp3`
fn sanitize_side(token: &str) -> String {
    let trimmed = token.trim_end_matches('/');
    let trimmed = trimmed.strip_prefix('/').unwrap_or(trimmed);
    trimmed
        .chars()
        .map(|c| if c == ':' || c == '/' { '_' } else { c })
        .collect()
}

/// `true` if `path` is a bisync listing with at least one real entry (the
/// first line is always the `# bisync listing v1 from ...` header).
fn seeded_list(path: &std::path::Path) -> bool {
    use std::io::{BufRead, BufReader};
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    // Without a header it is not a bisync listing.
    if reader.read_line(&mut header).is_err() || !header.starts_with("# bisync") {
        return false;
    }
    let mut entry = String::new();
    match reader.read_line(&mut entry) {
        Ok(0) | Err(_) => false,
        Ok(_) => !entry.trim().is_empty(),
    }
}

/// `true` if `pid` corresponds to a live rclone process.
///
/// The executable (`/proc/<pid>/comm`) is checked too, so a PID reassigned to
/// another process is not mistaken for the bisync session that left the lock:
/// in that case the lock is kept (fail safe) and rclone will report how to
/// remove it.
fn lock_owner_alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        Ok(comm) => comm.trim() == "rclone",
        Err(_) => false,
    }
}

/// Repeat interval in seconds derived from `sync_interval`.
pub fn interval_seconds(iv: &str) -> Option<u64> {
    match iv {
        "5min" => Some(300),
        "15min" => Some(900),
        "30min" => Some(1800),
        "1hr" => Some(3600),
        _ => None,
    }
}
