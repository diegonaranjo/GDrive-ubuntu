//! Wrappers de `rclone` (subprocesos) y parsing de su salida.
//!
//! Reimplementa `rclone_about`, `rclone_remote_exists`, `rclone_user_email`,
//! `rclone_is_authorized`, `rclone_lsd` y las utilidades de parseo de stats de
//! `gdrive/helpers.py` + `gdrive/sync.py`.

use std::process::Command;

use serde_json::Value;

use crate::constants::rclone_remote;

/// Salida de `rclone about --json` como `serde_json::Value`, o `None` si falla.
pub fn rclone_about() -> Option<Value> {
    let out = run_ok(&["about", &rclone_remote(), "--json"], 10)?;
    serde_json::from_str(&out).ok()
}

/// `true` si el remote (default `gdrive:`) está listado por `rclone listremotes`.
pub fn rclone_remote_exists(remote: Option<&str>) -> bool {
    let default = rclone_remote();
    let remote = remote.unwrap_or(&default);
    let name = remote.trim_end_matches(':');
    let Ok(out) = run(&["listremotes"], 5) else {
        return false;
    };
    out.lines().any(|l| l.trim().trim_end_matches(':') == name)
}

/// Obtiene el email de la cuenta conectada al remote, o `None`.
pub fn rclone_user_email() -> Option<String> {
    let remote = rclone_remote();

    // 1) Leer user/account/email del config (otros backends).
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

/// `true` si el remote existe y responde a `rclone about` (token válido).
pub fn rclone_is_authorized() -> bool {
    if !rclone_remote_exists(None) {
        return false;
    }
    run_ok(&["about", &rclone_remote(), "--json"], 30).is_some()
}

/// Lista de carpetas de primer nivel (`rclone lsd <remote>`) en orden lexicográfico.
pub fn rclone_lsd(remote: &str) -> Vec<String> {
    let Ok(out) = run(&["lsd", remote], 30) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| l.split_whitespace().last())
        .map(|s| s.to_string())
        .collect()
}

/// Cierra sesión: elimina el remote del config (`rclone config delete`).
pub fn rclone_sign_out() -> bool {
    let name = rclone_remote().trim_end_matches(':').to_string();
    run_ok(&["config", "delete", &name], 10).is_some()
}

/// Ejecuta rclone y devuelve stdout sólo si el exit code es 0.
fn run_ok(args: &[&str], timeout_secs: u64) -> Option<String> {
    let out = run(args, timeout_secs).ok()?;
    Some(out)
}

/// Ejecuta `rclone` con timeout (matando el proceso si excede).
fn run(args: &[&str], timeout_secs: u64) -> Result<String, String> {
    let mut cmd = Command::new("rclone");
    cmd.args(args);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(format!("rclone no disponible: {e}")),
    };

    // Timeout manual via poll + kill (std::process no expone timeout portable).
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

/// Parsea la línea de stats de rclone y devuelve `(transferido, total)` en bytes.
///
/// Dos formatos, ambos observados en salida real de rclone v1.74:
/// - con etiqueta (el que consumía el Python): `Transferred: 123 / 456`
/// - `--stats-one-line -v`: `2026-09-30 22:05:21 INFO  :   454.328 MiB / 1.045 GiB, 42%, 0 B/s, ETA -`
///
/// El segundo es el que realmente produce rclone con nuestras flags, y trae
/// decimales y **unidades distintas a cada lado** (`MiB` vs `GiB`); por eso el
/// valor se normaliza a bytes en vez de leer solo dígitos.
pub fn parse_stats(line: &str) -> Option<(u64, u64)> {
    let cleaned = strip_timestamp(line);
    if let Some(m) = find_pair(cleaned, "Transferred:") {
        return Some(m);
    }
    size_pair(cleaned)
}

/// Lee el par `{valor} [unidad] / {valor} [unidad]` de una línea de stats
/// compacta. No hace falta distinguirla de `Checks: 65000 / 65000` porque con
/// `--stats-one-line` rclone emite únicamente la línea de bytes (verificado
/// contra salida real: cero líneas `Checks:`).
fn size_pair(line: &str) -> Option<(u64, u64)> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let slash = toks.iter().position(|t| matches!(*t, "/" | "/,"))?;

    // Izquierda: `{num} {unidad} /` o `{num} /`.
    let left = slash
        .checked_sub(2)
        .and_then(|i| size_value(toks[i], toks[i + 1]))
        .or_else(|| slash.checked_sub(1).and_then(|i| size_value(toks[i], "")))?;

    // Derecha: `/ {num} {unidad}` o `/ {num}`.
    let right = toks
        .get(slash + 1)
        .and_then(|n| toks.get(slash + 2).and_then(|u| size_value(n, u)))
        .or_else(|| toks.get(slash + 1).and_then(|n| size_value(n, "")))?;

    Some((left, right))
}

/// Convierte `("1.045", "GiB,")` a bytes. `unidad` vacío significa bytes.
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

/// Redondea un conteo en bytes, rechazando NaN e infinitos: `1.045 * GiB` llega
/// como `f64` y puede desbordar o no ser finito según la salida de rclone.
fn to_bytes(bytes: f64) -> Option<u64> {
    if !bytes.is_finite() || bytes < 0.0 || bytes > u64::MAX as f64 {
        return None;
    }
    Some(bytes.round() as u64)
}

/// Número decimal sin separadores de miles; tolera la coma de fin de campo.
fn parse_decimal(tok: &str) -> Option<f64> {
    let tok = tok.trim_end_matches(',');
    if tok.is_empty() || !tok.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    tok.parse::<f64>().ok()
}

fn strip_timestamp(line: &str) -> &str {
    // Patrón: `yyyy-mm-dd` (formato real de v1.74) o `yyyy/mm/dd` (histórico)
    // seguido de `HH:MM:SS`.
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
    // Buscar `dd:dd:dd` (dos dígitos, `:`, dos dígitos, `:`, dos dígitos).
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
    // esperar ` / ` (puede llevar coma al final si no hay espacio)
    let slash = nums.next()?;
    if slash.trim_end_matches(',') != "/" {
        return None;
    }
    let b = parse_num_token(nums.next()?)?;
    Some((a, b))
}

/// Parsea un token numérico ignorando sufijos de puntuación (coma, `%`, etc.),
/// p. ej. `"456,"` → `456`, `"123"` → `123`.
fn parse_num_token(tok: &str) -> Option<u64> {
    let num: String = tok.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse::<u64>().ok()
}

/// Parsea velocidad, p. ej. `15.2 MiB/s`, `800 kBytes/s`, `4 M/s`.
/// Devuelve el token (número + unidad) completo, p. ej. `"15.2 MiB/s"`.
pub fn parse_speed(line: &str) -> Option<String> {
    let cleaned = strip_timestamp(line);
    let toks: Vec<&str> = cleaned.split_whitespace().collect();
    for i in 0..toks.len() {
        let tok = toks[i];
        // Caso 1: token compacto que termina en "/s" y empieza por dígito: "15.2MiB/s".
        if tok.ends_with("/s") || tok.ends_with("/s,") {
            let base = tok.trim_end_matches(',');
            if base.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return Some(base.to_string());
            }
        }
        // Caso 2: número seguido de unidad "/s": "15.2" "MiB/s".
        if tok.chars().all(|c| c.is_ascii_digit() || c == '.') && i + 1 < toks.len() {
            let next = toks[i + 1].trim_end_matches(',');
            if next.ends_with("/s") {
                return Some(format!("{tok} {next}"));
            }
        }
    }
    None
}
