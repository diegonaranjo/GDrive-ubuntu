//! Wrappers for `rclone` (subprocesses) and parsing of its output.
//!
//! Reimplements `rclone_about`, `rclone_remote_exists`, `rclone_user_email`,
//! `rclone_is_authorized`, `rclone_lsd` and the stats parsing utilities from
//! `gdrive/helpers.py` and the legacy `gdrive/sync.py` (removed in 1.1.0).

use std::process::Command;

use serde_json::Value;

use crate::constants::rclone_remote;

/// Output of `rclone about --json` as `serde_json::Value`, or `None` on error.
pub fn rclone_about() -> Option<Value> {
    let out = run_ok(&["about", &rclone_remote(), "--json"], 10)?;
    serde_json::from_str(&out).ok()
}

/// `true` if the remote (default `gdrive:`) is listed by `rclone listremotes`.
pub fn rclone_remote_exists(remote: Option<&str>) -> bool {
    let default = rclone_remote();
    let remote = remote.unwrap_or(&default);
    let name = remote.trim_end_matches(':');
    let Ok(out) = run(&["listremotes"], 5) else {
        return false;
    };
    out.lines().any(|l| l.trim().trim_end_matches(':') == name)
}

/// Gets the email of the account connected to the remote, or `None`.
pub fn rclone_user_email() -> Option<String> {
    let remote = rclone_remote();

    // 1) Read user/account/email from the config (other backends).
    if let Ok(out) = run(&["config", "show", &remote], 5) {
        for line in out.lines() {
            let line = line.trim();
            for key in ["user", "account", "email"] {
                if let Some(rest) = line.strip_prefix(&format!("{key} =")) {
                    let val = rest.trim();
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }

    // 2) Google Drive: `rclone backend user <remote>` → JSON.
    if let Ok(out) = run(&["backend", "user", &remote], 10) {
        if let Ok(v) = serde_json::from_str::<Value>(&out) {
            if let Some(e) = v.get("email").or_else(|| v.get("userEmailAddress")) {
                if let Some(s) = e.as_str() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

/// `true` if the remote exists and answers `rclone about` (valid token).
pub fn rclone_is_authorized() -> bool {
    if !rclone_remote_exists(None) {
        return false;
    }
    run_ok(&["about", &rclone_remote(), "--json"], 30).is_some()
}

/// List of first-level folders (`rclone lsd <remote>`) in lexicographic order.
pub fn rclone_lsd(remote: &str) -> Vec<String> {
    let Ok(out) = run(&["lsd", remote], 30) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| l.split_whitespace().last())
        .map(|s| s.to_string())
        .collect()
}

/// Signs out: deletes the remote from the config (`rclone config delete`).
pub fn rclone_sign_out() -> bool {
    let name = rclone_remote().trim_end_matches(':').to_string();
    run_ok(&["config", "delete", &name], 10).is_some()
}

/// Runs rclone and returns stdout only if the exit code is 0.
fn run_ok(args: &[&str], timeout_secs: u64) -> Option<String> {
    let out = run(args, timeout_secs).ok()?;
    Some(out)
}

/// Runs `rclone` with a timeout (killing the process when it exceeds it).
fn run(args: &[&str], timeout_secs: u64) -> Result<String, String> {
    let mut cmd = Command::new("rclone");
    cmd.args(args);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(format!("rclone no disponible: {e}")),
    };

    // Manual timeout via poll + kill (std::process has no portable timeout).
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    let out = child
                        .wait_with_output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                        .map_err(|e| e.to_string())?;
                    return Ok(out);
                }
                return Err(format!("rclone exit {status}"));
            }
            Ok(None) => {
                if start.elapsed().as_secs() > timeout_secs {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("timeout".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Parses rclone's stats line and returns `(transferred, total)` in bytes.
///
/// Two formats, both observed in real rclone v1.74 output:
/// - labelled (the one Python consumed): `Transferred: 123 / 456`
/// - `--stats-one-line -v`: `2026-09-30 22:05:21 INFO  :   454.328 MiB / 1.045 GiB, 42%, 0 B/s, ETA -`
///
/// The second is what rclone actually produces with our flags, and it brings
/// decimals and **different units on each side** (`MiB` vs `GiB`); that is why
/// the value is normalized to bytes instead of reading digits only.
pub fn parse_stats(line: &str) -> Option<(u64, u64)> {
    let cleaned = strip_timestamp(line);
    if let Some(m) = find_pair(cleaned, "Transferred:") {
        return Some(m);
    }
    size_pair(cleaned)
}

/// Reads the `{value} [unit] / {value} [unit]` pair from a compact stats
/// line. No need to tell it apart from `Checks: 65000 / 65000` because with
/// `--stats-one-line` rclone emits only the bytes line (verified against
/// real output: zero `Checks:` lines).
fn size_pair(line: &str) -> Option<(u64, u64)> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let slash = toks.iter().position(|t| matches!(*t, "/" | "/,"))?;

    // Left: `{num} {unit} /` or `{num} /`.
    let left = slash
        .checked_sub(2)
        .and_then(|i| size_value(toks[i], toks[i + 1]))
        .or_else(|| slash.checked_sub(1).and_then(|i| size_value(toks[i], "")))?;

    // Right: `/ {num} {unit}` or `/ {num}`.
    let right = toks
        .get(slash + 1)
        .and_then(|n| toks.get(slash + 2).and_then(|u| size_value(n, u)))
        .or_else(|| toks.get(slash + 1).and_then(|n| size_value(n, "")))?;

    Some((left, right))
}

/// Converts `("1.045", "GiB,")` to bytes. An empty `unit` means bytes.
fn size_value(num: &str, unit: &str) -> Option<u64> {
    let unit = unit.trim_end_matches([',', ')']);
    let mult = if unit.is_empty() {
        1.0
    } else {
        match unit.to_ascii_lowercase().as_str() {
            "b" => 1.0,
            "k" | "kb" | "kib" => 1024.0,
            "m" | "mb" | "mib" => 1024.0 * 1024.0,
            "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
            "t" | "tb" | "tib" => 1024.0f64.powi(4),
            "p" | "pb" | "pib" => 1024.0f64.powi(5),
            _ => return None,
        }
    };
    to_bytes(parse_decimal(num)? * mult)
}

/// Rounds a byte count, rejecting NaN and infinities: `1.045 * GiB` arrives
/// as `f64` and may overflow or be non-finite depending on rclone's output.
fn to_bytes(bytes: f64) -> Option<u64> {
    if !bytes.is_finite() || bytes < 0.0 || bytes > u64::MAX as f64 {
        return None;
    }
    Some(bytes.round() as u64)
}

/// Decimal number without thousands separators; tolerates the trailing comma.
fn parse_decimal(tok: &str) -> Option<f64> {
    let tok = tok.trim_end_matches(',');
    if tok.is_empty() || !tok.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    tok.parse::<f64>().ok()
}

fn strip_timestamp(line: &str) -> &str {
    // Pattern: `yyyy-mm-dd` (real v1.74 format) or `yyyy/mm/dd` (historical)
    // followed by `HH:MM:SS`.
    let b = line.as_bytes();
    let date_sep = |i: usize| b[i] == b'-' || b[i] == b'/';
    if b.len() >= 10 && date_sep(4) && date_sep(7) && b[..4].iter().all(u8::is_ascii_digit) {
        if let Some(idx) = find_hms(line) {
            return &line[idx..];
        }
    }
    line
}

fn find_hms(s: &str) -> Option<usize> {
    // Find `dd:dd:dd` (two digits, `:`, two digits, `:`, two digits).
    let b = s.as_bytes();
    for i in 0..b.len().saturating_sub(8) {
        if b[i..i + 2].iter().all(u8::is_ascii_digit)
            && b[i + 2] == b':'
            && b[i + 3..i + 5].iter().all(u8::is_ascii_digit)
            && b[i + 5] == b':'
            && b[i + 6..i + 8].iter().all(u8::is_ascii_digit)
        {
            return Some(i + 8);
        }
    }
    None
}

fn find_pair(cleaned: &str, marker: &str) -> Option<(u64, u64)> {
    let rest = cleaned.split_once(marker)?.1;
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    let mut nums = rest.split_whitespace();
    let a = parse_num_token(nums.next()?)?;
    // expect ` / ` (may carry a trailing comma when there is no space)
    let slash = nums.next()?;
    if slash.trim_end_matches(',') != "/" {
        return None;
    }
    let b = parse_num_token(nums.next()?)?;
    Some((a, b))
}

/// Parses a numeric token ignoring punctuation suffixes (comma, `%`, etc.),
/// e.g. `"456,"` → `456`, `"123"` → `123`.
fn parse_num_token(tok: &str) -> Option<u64> {
    let num: String = tok.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse::<u64>().ok()
}

/// Parses a speed, e.g. `15.2 MiB/s`, `800 kBytes/s`, `4 M/s`.
/// Returns the full token (number + unit), e.g. `"15.2 MiB/s"`.
pub fn parse_speed(line: &str) -> Option<String> {
    let cleaned = strip_timestamp(line);
    let toks: Vec<&str> = cleaned.split_whitespace().collect();
    for i in 0..toks.len() {
        let tok = toks[i];
        // Case 1: compact token ending in "/s", starting with a digit: "15.2MiB/s".
        if tok.ends_with("/s") || tok.ends_with("/s,") {
            let base = tok.trim_end_matches(',');
            if base.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return Some(base.to_string());
            }
        }
        // Case 2: number followed by a "/s" unit: "15.2" "MiB/s".
        if tok.chars().all(|c| c.is_ascii_digit() || c == '.') && i + 1 < toks.len() {
            let next = toks[i + 1].trim_end_matches(',');
            if next.ends_with("/s") {
                return Some(format!("{tok} {next}"));
            }
        }
    }
    None
}
