//! SyncEngine: orquesta `rclone bisync` en segundo plano.
//!
//! El nombre de los listados `.lst`/`.lck` de bisync se **deriva del par
//! (`remote`, `sync_dir`)** con el mismo saneado que aplica rclone, en lugar de
//! estar hardcodeado a una ruta concreta del disco del autor.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::constants::{bisync_cache_dir, rclone_remote, sync_dir_default};
use crate::error::{Error, Result};
use crate::logging::{log_write, Level};
use crate::rclone::parse_stats;

/// Máximo tiempo sin recibir ninguna línea de rclone antes de dar el sync por
/// colgado y terminarlo.
///
/// Sustituye al antiguo timeout de pared (600 s): con decenas de miles de
/// archivos un sync legítimo tarda más de 10 min, y matarlo a mitad dejaba a
/// bisync sin listados válidos y exigiendo un `--resync` que pisaba los cambios
/// locales. Mientras hay actividad, rclone emite una línea de `--stats` cada 2 s
/// y además se auto-termina en atascos de red (`--contimeout`/`--timeout`, 5 min
/// por defecto), así que el silencio sostenido es la única señal fiable de que
/// el proceso no va a seguir solo.
pub const SYNC_IDLE_TIMEOUT: Duration = Duration::from_secs(900);

/// Estado del motor de sincronización.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// Sin sincronización en curso.
    Idle,
    /// Sincronización en curso.
    Syncing,
    /// Sincronización pausada por el usuario.
    Paused,
    /// La última sincronización falló.
    Error,
}

impl SyncStatus {
    /// Representación textual (estable) del estado, usada por D-Bus y logs.
    pub fn as_str(self) -> &'static str {
        match self {
            SyncStatus::Idle => "idle",
            SyncStatus::Syncing => "syncing",
            SyncStatus::Paused => "paused",
            SyncStatus::Error => "error",
        }
    }
}

/// Entidad que arma y ejecuta los comandos de bisync. Sin estado global mutable
/// compartido: cada `run` es autocontenida y devuelve un resultado `Result`,
/// eliminando las condiciones de carrera del `SyncManager` Python.
#[derive(Debug, Clone, Default)]
pub struct SyncEngine {
    /// Configuración que gobierna la sincronización.
    pub config: Config,
}

/// Resultado de una sincronización completada.
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    /// `true` si la sincronización terminó con éxito.
    pub ok: bool,
    /// Mensaje descriptivo (para logs y UI).
    pub message: String,
    /// Bytes transferidos (best-effort, desde las stats de rclone).
    pub transferred: u64,
    /// Bytes totales esperados (best-effort).
    pub total: u64,
}

impl SyncEngine {
    /// Crea un motor a partir de una [`Config`].
    pub fn new(config: Config) -> Self {
        SyncEngine { config }
    }

    /// Construye el comando `rclone bisync` aplicando la configuración de forma
    /// completa (a diferencia del Python, que ignoraba la mayoría de settings).
    pub fn build_cmd(&self) -> (Vec<String>, PathBuf) {
        let sync_dir = self.sync_dir();
        let mut cmd: Vec<String> = vec!["rclone".into(), "bisync".into()];

        cmd.push(rclone_remote());
        cmd.push(sync_dir.to_string_lossy().to_string());

        // Sembrar los listados solo la primera vez para este par. Ojo: `--resync`
        // a secas equivale a `--resync-mode path1`, y Path1 es el remoto, así que
        // un resync implícito reemplaza los archivos locales por los de Drive
        // aunque los locales sean más recientes. `newer` deja ganar a la copia
        // modificada más recientemente, que es lo que espera el usuario.
        if !self.has_list_files() {
            cmd.push("--resync".into());
            cmd.push("--resync-mode".into());
            cmd.push("newer".into());
        }

        // `--recover` retoma desde los listados del último run bueno en lugar de
        // exigir un resync completo, y `--resilient` permite que el *siguiente*
        // run reintente tras un error (p. ej. un archivo de Drive con
        // `403 cannotDownloadAbusiveFile`) sin haber destruido la línea base.
        cmd.push("--recover".into());
        cmd.push("--resilient".into());

        // Conflicto = el archivo cambió en ambos lados desde el último sync. Sin
        // esto bisync aborta el run y deja el estado pidiendo `--resync`. El
        // perdedor no se borra: rclone lo renombra (`--conflict-loser num`).
        cmd.push("--conflict-resolve".into());
        cmd.push(self.conflict_resolution().to_string());

        // Aplicar `sync_mode`: `selective` restringe la sincronización a las
        // carpetas seleccionadas (`selected_folders`), añadiendo `--include`
        // para limitar el árbol a esas rutas (y sus descendientes).
        for inc in self.selective_includes() {
            cmd.push("--include".into());
            cmd.push(inc);
        }

        // `-v` no es decorativo: sin verbosity rclone **no emite ni una línea**
        // (verificado: 0 bytes de salida al copiar 1 GiB), así que la UI se queda
        // sin progreso y el watchdog de inactividad no tiene de dónde contar.
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

        // Lista el remoto de forma recursiva en una sola pasada en lugar de una
        // llamada por directorio. Medido en Drive sobre un subárbol real: 18,74 s
        // sin el flag, 1,12 s con él. Acá eso recorta el sync completo (listar
        // ~54k archivos llevaba 15-20 min) y sobre todo baja las llamadas a la
        // API, que es lo que producía los abortos `rateLimitExceeded` del log.
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

    /// Ejecuta la sincronización de forma síncrona y devuelve el resultado.
    /// El proceso se mata si deja de emitir salida durante `SYNC_IDLE_TIMEOUT`.
    pub fn run(&self) -> Result<SyncOutcome> {
        self.run_with_callback(|_line, _tr, _to, _speed| {})
    }

    /// Como [`run`](Self::run), pero invoca un callback por cada línea de salida de
    /// rclone con su progreso `(transferred, total)` y velocidad parseados. Esto
    /// permite al daemon reenviar el progreso a la UI en tiempo real.
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

        // Limpiar lock huérfano si rclone bisync ya no corre.
        self.clear_orphan_lock();

        let mut child = Command::new("rclone")
            .args(&cmd[1..])
            // stdout al vacío: rclone escribe sus logs y las líneas de `--stats`
            // por **stderr**. El código anterior dejaba stderr como pipe sin
            // leerlo nunca: con decenas de miles de archivos el buffer de 64 KB
            // se llenaba y rclone quedaba bloqueado a la mitad del sync, mientras
            // el bucle de stdout no recibía línea alguna (ni progreso, ni forma
            // de que se evaluara el timeout).
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

        // Sin `fix_lst_files()`: rclone ya promueve `*.lst-new` → `*.lst` al
        // terminar un run con éxito. Hacerlo además desde fuera renombraba los
        // listados **incompletos** de un run abortado sobre la línea base buena,
        // dejándola vacía y forzando un `--resync` en cada sincronización.
        let ok = status.success();
        let message = if ok {
            crate::i18n::t("sync_manager.completed")
        } else {
            // stderr ya se consumió en streaming: el detalle sale del `tail`,
            // priorizando la última línea ERROR que explicó el fallo.
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

        // El camino de éxito no dejaba ningún rastro de rclone en el log, así que
        // un run que "completa" en el mismo segundo es indeterminable. La última
        // línea de stats dice cuánto se movió de verdad (o que no se movió nada).
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

    /// Carpeta de sync resuelta (default `$HOME/GoogleDrive`).
    pub fn sync_dir(&self) -> PathBuf {
        if self.config.sync_dir.is_empty() {
            sync_dir_default()
        } else {
            PathBuf::from(&self.config.sync_dir)
        }
    }

    /// Globs `--include` derivados del `sync_mode` activo.
    ///
    /// # Mapeo de modos de sincronización
    ///
    /// - `mirror` (default): sincronización bidireccional completa del árbol.
    ///   No añade filtros; `bisync` compara todo `sync_dir` con el remoto.
    /// - `selective`: restringe la sincronización a las carpetas elegidas en
    ///   `config.selected_folders`. Por cada carpeta se genera un `--include`
    ///   `carpeta/**` para incluirlo a ella y sus descendientes (excluyendo el
    ///   resto con el `--exclude` global implícito de rclone).
    /// - `stream`: acceso bajo demanda (sin descarga completa). **No soportado
    ///   por el motor `bisync` actual**; se comporta como `mirror` mientras no
    ///   se implemente un backend de `rclone mount`. Documentado para claridad.
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

    /// Compone UN solo `--bwlimit` a partir de los límites configurados.
    ///
    /// rclone v1.74 no entiende sintaxis direccional: `--bwlimit u:1M` y
    /// `--bwlimit d:500k` devuelven `invalid argument` (medido), y repetir el
    /// flag hace que solo valga el último (medido: `--bwlimit 1M --bwlimit 2M`
    /// copa a 2 MB/s). El código anterior emitía hasta tres flags, así que
    /// configurar "límite de descarga" rompía cada sincronización.
    ///
    /// Se aplica el **más estricto** de los valores pedidos, de forma que ningún
    /// cap configurado se supere. Si hay valores pero ninguno parsea, se pasa el
    /// primero tal cual: mejor un error visible de rclone que sincronizar sin
    /// límite en silencio.
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

    /// Resolución de conflictos que se pasa a `--conflict-resolve`.
    ///
    /// El ajuste `conflict_action` venía con el valor histórico `"rename"`, que
    /// bisync no entiende, así que cualquier valor no reconocido cae en `newer`:
    /// gana la copia modificada más recientemente y el perdedor se renombra
    /// (`--conflict-loser num`), no se elimina.
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

    /// Prefijo con el que rclone bisync nombra sus listados y su lock.
    ///
    /// Es `{lado1}..{lado2}`, donde cada lado es el token `remote:path` saneado
    /// como lo hace rclone. Verificado contra rclone v1.74 con tres formas
    /// distintas de ruta (ver `sanitize_side`), p. ej.
    /// `gdrive:` + `/home/usuario/GoogleDrive` →
    /// `gdrive_..home_usuario_GoogleDrive`.
    pub fn bisync_prefix(&self) -> String {
        let path1 = rclone_remote();
        let path2 = self.sync_dir().to_string_lossy().to_string();
        format!("{}..{}", sanitize_side(&path1), sanitize_side(&path2))
    }

    /// `true` si existe una línea base de bisync **usable** para este par.
    ///
    /// No basta con que los archivos estén: un listado abortado a mitad deja un
    /// `.lst` con solo la línea de cabecera, y tratarlo como línea base es lo que
    /// hacía que bisync declarara cada archivo como conflicto y el run exigiera
    /// `--resync`. Se exige al menos una entrada real por lado.
    pub fn has_list_files(&self) -> bool {
        let prefix = self.bisync_prefix();
        let dir = bisync_cache_dir();
        ["path1", "path2"]
            .iter()
            .all(|side| seeded_list(&dir.join(format!("{prefix}.{side}.lst"))))
    }

    /// Elimina el lock si el proceso que lo creó ya no existe (lock huérfano).
    ///
    /// rclone **no** autorresuelve un lock abandonado: se niega a sincronizar y
    /// avisa que hay que borrarlo a mano, así que sin esto un sync interrumpido
    /// bloquearía el daemon para siempre. Se lee el PID del propio lock en lugar
    /// de un `pgrep -f` porque es exacto: borrar el lock de una sesión viva
    /// permitiría dos bisync concurrentes sobre el mismo par, y eso corrompe los
    /// listados.
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
            // Lock ilegible: no se borra a ciegas. rclone rechazará el run y su
            // mensaje dirá qué archivo quitar.
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

    /// Ruta del archivo de lock de bisync para el `sync_dir` actual.
    pub fn lock_path(&self) -> PathBuf {
        bisync_cache_dir().join(format!("{}.lck", self.bisync_prefix()))
    }
}

/// Convierte un límite de ancho de banda (`"500k"`, `"1M"`, `"10M"`, `"1G"`, o
/// un número suelto que rclone interpreta como KiB/s) a bytes por segundo.
/// `None` si el valor no es reconocible.
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

/// Sanea un token `remote:path` para el nombre de los listados de bisync, como
/// lo hace rclone: recorta el `/` inicial de una ruta absoluta y el `/` final de
/// la raíz de un remoto, y convierte `:` y `/` en `_`.
///
/// Casos verificados contra rclone v1.74:
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

/// `true` si `path` es un listado de bisync con al menos una entrada real
/// (la primera línea es siempre la cabecera `# bisync listing v1 from ...`).
fn seeded_list(path: &std::path::Path) -> bool {
    use std::io::{BufRead, BufReader};
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    // Sin cabecera no es un listado de bisync.
    if reader.read_line(&mut header).is_err() || !header.starts_with("# bisync") {
        return false;
    }
    let mut entry = String::new();
    match reader.read_line(&mut entry) {
        Ok(0) | Err(_) => false,
        Ok(_) => !entry.trim().is_empty(),
    }
}

/// `true` si `pid` corresponde a un proceso rclone vivo.
///
/// Se comprueba también el ejecutable (`/proc/<pid>/comm`) para no confundir un
/// PID reasignado a otro proceso con la sesión bisync que dejó el lock: en ese
/// caso se conserva el lock (fallar hacia lo seguro) y rclone avisará de cómo
/// quitarlo.
fn lock_owner_alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        Ok(comm) => comm.trim() == "rclone",
        Err(_) => false,
    }
}

/// Intervalo de repetición en segundos a partir de `sync_interval`.
pub fn interval_seconds(iv: &str) -> Option<u64> {
    match iv {
        "5min" => Some(300),
        "15min" => Some(900),
        "30min" => Some(1800),
        "1hr" => Some(3600),
        _ => None,
    }
}
