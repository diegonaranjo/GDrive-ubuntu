//! Minimal i18n for the UI (Spanish + English), mirroring `gdrive/i18n.py`.
//!
//! Keys and values match `gdrive/i18n.py` 1:1 so translations stay in sync with
//! the Python source during the transition. The catalog covers all six pages.

use std::collections::HashMap;

/// Supported UI languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Spanish (default).
    Es,
    /// English (selected via config).
    En,
}

/// Returns the language code (`"es"`/`"en"`) for a [`Lang`].
pub fn code(lang: Lang) -> &'static str {
    match lang {
        Lang::Es => "es",
        Lang::En => "en",
    }
}

/// Returns the [`Lang`] for a language code (defaults to Spanish).
pub fn from_code(code: &str) -> Lang {
    match code {
        "en" => Lang::En,
        _ => Lang::Es,
    }
}

/// Translates `key` for `lang`, falling back to the key itself if missing.
pub fn t(lang: Lang, key: &'static str) -> &'static str {
    table(lang).get(key).copied().unwrap_or(key)
}

/// Translates `key` and interpolates `{placeholder}` tokens from `args`
/// (flat `(name, value)` pairs). Returns the key unchanged if not found.
pub fn tf(lang: Lang, key: &'static str, args: &[(&str, &str)]) -> String {
    let mut out = t(lang, key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Returns the translation table for the given language.
fn table(lang: Lang) -> &'static HashMap<&'static str, &'static str> {
    match lang {
        Lang::Es => &ES,
        Lang::En => &EN,
    }
}

/// Spanish translations.
static ES: std::sync::LazyLock<HashMap<&'static str, &'static str>> =
    std::sync::LazyLock::new(|| {
        HashMap::from([
            // Status page
            ("status.idle", "Listo"),
            ("status.syncing", "Sincronizando…"),
            ("status.paused", "Pausado"),
            ("status.error", "Error"),
            ("status.detail.idle", "Google Drive está al día"),
            (
                "status.detail.syncing",
                "Sincronizando archivos con Google Drive",
            ),
            ("status.detail.paused", "La sincronización está pausada"),
            (
                "status.detail.error",
                "Hubo un error durante la sincronización",
            ),
            ("status.info_group", "Información"),
            ("status.last_sync", "Última sincronización"),
            ("status.last_sync_value", "Nunca"),
            ("status.files_synced", "Archivos sincronizados"),
            ("status.errors", "Errores"),
            ("status.speed", "Velocidad de transferencia"),
            ("status.sync_now", "Sincronizar ahora"),
            ("status.pause", "Pausar"),
            ("status.resume", "Reanudar"),
            // Account page
            ("account.title", "Cuenta"),
            ("account.not_signed_in", "Not signed in"),
            ("account.google_drive_account", "Google Drive account"),
            ("account.storage", "Almacenamiento"),
            ("account.storage_used", "{used} de {total} usados"),
            ("account.no_storage_info", "No se pudo obtener información"),
            ("account.no_limit", "Sin límite"),
            ("account.actions", "Acciones"),
            ("account.open_drive", "Abrir Google Drive"),
            ("account.manage_account", "Gestionar cuenta"),
            ("account.open", "Abrir"),
            ("account.refresh_info", "Actualizar información"),
            // Activity page
            ("activity.title", "Actividad"),
            ("activity.clear", "Limpiar"),
            ("activity.search_placeholder", "Buscar en actividad…"),
            ("activity.no_activity", "Sin actividad"),
            // Selective sync page
            ("selective.title", "Carpetas"),
            ("selective.refresh", "Actualizar"),
            ("selective.no_folders", "No hay carpetas"),
            // Sync settings page
            ("sync.title", "Sincronización"),
            ("sync.location", "Ubicación"),
            ("sync.sync_folder", "Carpeta de sincronización"),
            ("sync.change", "Cambiar"),
            ("sync.mode", "Modo"),
            ("sync.file_mode", "Modo de archivos"),
            ("sync.file_mode_subtitle", "Cómo se almacenan los archivos"),
            ("sync.mode_mirror", "Espejo — Copia completa"),
            ("sync.mode_selective", "Puntual — Solo seleccionados"),
            ("sync.scheduling", "Programación"),
            ("sync.interval", "Intervalo"),
            ("sync.interval_subtitle", "Frecuencia de sincronización"),
            ("sync.interval_manual", "Manual"),
            ("sync.interval_5min", "5 minutos"),
            ("sync.interval_15min", "15 minutos"),
            ("sync.interval_30min", "30 minutos"),
            ("sync.interval_1hr", "1 hora"),
            ("sync.watch_changes", "Detección en tiempo real"),
            (
                "sync.watch_subtitle",
                "Sincronizar automáticamente al detectar cambios",
            ),
            ("sync.pause_sync", "Pausar sincronización"),
            ("sync.pause_subtitle", "Detener sincronización automática"),
            ("sync.bandwidth", "Ancho de banda"),
            ("sync.global_limit", "Límite global"),
            ("sync.no_limit", "Vacío = sin límite"),
            ("sync.placeholder_bw", "Ej: 1M"),
            ("sync.advanced", "Avanzado"),
            ("sync.skip_hidden", "Omitir ocultos"),
            (
                "sync.store_metadata",
                "Guardar metadatos (rclone --metadata)",
            ),
            ("sync.transfer_threads", "Hilos de transferencia"),
            ("sync.shortcuts", "Accesos"),
            ("sync.open_local", "Abrir carpeta local"),
            ("sync.open_browser", "Abrir en navegador"),
            ("sync.open", "Abrir"),
            ("sync.select_folder", "Seleccionar carpeta"),
            ("sync.config_saved", "Configuración guardada"),
            // Settings page
            ("settings.title", "Ajustes"),
            ("settings.general", "General"),
            ("settings.notifications", "Notificaciones"),
            ("settings.startup", "Iniciar con el sistema"),
            ("settings.nautilus", "Integrar en Nautilus"),
            ("settings.language", "Idioma"),
            ("settings.language_subtitle", "Idioma de la interfaz"),
            ("settings.network", "Red"),
            ("settings.use_proxy", "Usar proxy"),
            ("settings.proxy_url", "URL del proxy"),
            ("settings.cache", "Caché"),
            ("settings.clear_cache", "Limpiar caché"),
            ("settings.cache_cleared", "Caché limpiada"),
            ("settings.security", "Seguridad"),
            ("settings.encryption", "Cifrado en reposo"),
            ("settings.logs", "Registros (Logs)"),
            ("settings.enable_logs", "Habilitar registros"),
            (
                "settings.enable_logs_subtitle",
                "Guardar actividad en archivos de log",
            ),
            ("settings.log_level", "Nivel de registro"),
            ("settings.log_level_subtitle", "Información a registrar"),
            ("settings.log_level_error", "Solo errores"),
            ("settings.log_level_info", "Información"),
            ("settings.log_level_debug", "Depuración"),
            ("settings.log_level_trace", "Traza completa"),
            ("settings.log_rotate", "Rotación automática"),
            ("settings.log_rotate_subtitle", "Comprimir logs antiguos"),
            ("settings.log_max_size", "Tamaño máximo por archivo (MB)"),
            ("settings.log_max_files", "Máximo de archivos de log"),
            ("settings.log_files", "Archivos de registro"),
            ("settings.log_activity", "Actividad"),
            ("settings.log_rclone", "Rclone"),
            ("settings.log_debug", "Depuración"),
            ("settings.open", "Abrir"),
            ("settings.log_folder", "Carpeta de logs"),
            ("settings.open_folder", "Abrir carpeta"),
            ("settings.log_actions", "Acciones de registro"),
            ("settings.export_logs", "Exportar logs…"),
            ("settings.delete_all_logs", "Eliminar todos los logs"),
            ("settings.logs_exported", "Logs exportados"),
            ("settings.logs_deleted", "Logs eliminados"),
            ("settings.about", "Acerca de"),
            ("settings.version", "Versión"),
            ("settings.docs", "Documentación"),
            ("settings.reset_config", "Restablecer configuración"),
            ("settings.config_saved", "Configuración guardada"),
            ("settings.config_reset", "Configuración restablecida"),
            (
                "settings.reload_hint",
                "Recarga la app para aplicar todos los cambios",
            ),
            ("settings.export_logs_title", "Exportar logs"),
            ("settings.confirm_delete_logs", "¿Eliminar todos los logs?"),
            (
                "settings.confirm_delete_logs_body",
                "Esta acción no se puede deshacer.",
            ),
            ("settings.confirm_delete", "Eliminar"),
            ("settings.confirm_reset", "¿Restablecer configuración?"),
            (
                "settings.confirm_reset_body",
                "Volverás a los valores por defecto.",
            ),
            ("settings.confirm_reset_btn", "Restablecer"),
            // Sidebar
            ("sidebar.status", "Estado"),
            ("sidebar.account", "Cuenta"),
            ("sidebar.sync", "Sincronización"),
            ("sidebar.selective", "Carpetas"),
            ("sidebar.settings", "Configuración"),
            ("sidebar.activity", "Actividad"),
            // Widgets
            ("widget.cancel", "Cancelar"),
            ("widget.confirm", "Confirmar"),
        ])
    });

/// English translations.
static EN: std::sync::LazyLock<HashMap<&'static str, &'static str>> =
    std::sync::LazyLock::new(|| {
        HashMap::from([
            // Status page
            ("status.idle", "Ready"),
            ("status.syncing", "Syncing…"),
            ("status.paused", "Paused"),
            ("status.error", "Error"),
            ("status.detail.idle", "Google Drive is up to date"),
            ("status.detail.syncing", "Syncing files with Google Drive"),
            ("status.detail.paused", "Sync is paused"),
            ("status.detail.error", "An error occurred during sync"),
            ("status.info_group", "Information"),
            ("status.last_sync", "Last sync"),
            ("status.last_sync_value", "Never"),
            ("status.files_synced", "Files synced"),
            ("status.errors", "Errors"),
            ("status.speed", "Transfer speed"),
            ("status.sync_now", "Sync now"),
            ("status.pause", "Pause"),
            ("status.resume", "Resume"),
            // Account page
            ("account.title", "Account"),
            ("account.not_signed_in", "Not signed in"),
            ("account.google_drive_account", "Google Drive account"),
            ("account.storage", "Storage"),
            ("account.storage_used", "{used} of {total} used"),
            ("account.no_storage_info", "Could not retrieve information"),
            ("account.no_limit", "No limit"),
            ("account.actions", "Actions"),
            ("account.open_drive", "Open Google Drive"),
            ("account.manage_account", "Manage account"),
            ("account.open", "Open"),
            ("account.refresh_info", "Refresh information"),
            // Activity page
            ("activity.title", "Activity"),
            ("activity.clear", "Clear"),
            ("activity.search_placeholder", "Search activity…"),
            ("activity.no_activity", "No activity"),
            // Selective sync page
            ("selective.title", "Folders"),
            ("selective.refresh", "Refresh"),
            ("selective.no_folders", "No folders"),
            // Sync settings page
            ("sync.title", "Sync"),
            ("sync.location", "Location"),
            ("sync.sync_folder", "Sync folder"),
            ("sync.change", "Change"),
            ("sync.mode", "Mode"),
            ("sync.file_mode", "File mode"),
            ("sync.file_mode_subtitle", "How files are stored"),
            ("sync.mode_mirror", "Mirror — Full copy"),
            ("sync.mode_selective", "Selective — Selected only"),
            ("sync.scheduling", "Scheduling"),
            ("sync.interval", "Interval"),
            ("sync.interval_subtitle", "Sync frequency"),
            ("sync.interval_manual", "Manual"),
            ("sync.interval_5min", "5 minutes"),
            ("sync.interval_15min", "15 minutes"),
            ("sync.interval_30min", "30 minutes"),
            ("sync.interval_1hr", "1 hour"),
            ("sync.watch_changes", "Real-time detection"),
            (
                "sync.watch_subtitle",
                "Sync automatically when changes are detected",
            ),
            ("sync.pause_sync", "Pause sync"),
            ("sync.pause_subtitle", "Stop automatic sync"),
            ("sync.bandwidth", "Bandwidth"),
            ("sync.global_limit", "Global limit"),
            ("sync.no_limit", "Empty = no limit"),
            ("sync.placeholder_bw", "e.g. 1M"),
            ("sync.advanced", "Advanced"),
            ("sync.skip_hidden", "Skip hidden files"),
            ("sync.store_metadata", "Store metadata (rclone --metadata)"),
            ("sync.transfer_threads", "Transfer threads"),
            ("sync.shortcuts", "Shortcuts"),
            ("sync.open_local", "Open local folder"),
            ("sync.open_browser", "Open in browser"),
            ("sync.open", "Open"),
            ("sync.select_folder", "Select folder"),
            ("sync.config_saved", "Settings saved"),
            // Settings page
            ("settings.title", "Settings"),
            ("settings.general", "General"),
            ("settings.notifications", "Notifications"),
            ("settings.startup", "Start on system login"),
            ("settings.nautilus", "Integrate in Nautilus"),
            ("settings.language", "Language"),
            ("settings.language_subtitle", "Interface language"),
            ("settings.network", "Network"),
            ("settings.use_proxy", "Use proxy"),
            ("settings.proxy_url", "Proxy URL"),
            ("settings.cache", "Cache"),
            ("settings.clear_cache", "Clear cache"),
            ("settings.cache_cleared", "Cache cleared"),
            ("settings.security", "Security"),
            ("settings.encryption", "Encryption at rest"),
            ("settings.logs", "Logs"),
            ("settings.enable_logs", "Enable logging"),
            (
                "settings.enable_logs_subtitle",
                "Save activity to log files",
            ),
            ("settings.log_level", "Log level"),
            ("settings.log_level_subtitle", "Information to log"),
            ("settings.log_level_error", "Errors only"),
            ("settings.log_level_info", "Information"),
            ("settings.log_level_debug", "Debug"),
            ("settings.log_level_trace", "Full trace"),
            ("settings.log_rotate", "Auto rotation"),
            ("settings.log_rotate_subtitle", "Compress old logs"),
            ("settings.log_max_size", "Max file size (MB)"),
            ("settings.log_max_files", "Max log files"),
            ("settings.log_files", "Log files"),
            ("settings.log_activity", "Activity"),
            ("settings.log_rclone", "Rclone"),
            ("settings.log_debug", "Debug"),
            ("settings.open", "Open"),
            ("settings.log_folder", "Log folder"),
            ("settings.open_folder", "Open folder"),
            ("settings.log_actions", "Log actions"),
            ("settings.export_logs", "Export logs…"),
            ("settings.delete_all_logs", "Delete all logs"),
            ("settings.logs_exported", "Logs exported"),
            ("settings.logs_deleted", "Logs deleted"),
            ("settings.about", "About"),
            ("settings.version", "Version"),
            ("settings.docs", "Documentation"),
            ("settings.reset_config", "Reset settings"),
            ("settings.config_saved", "Settings saved"),
            ("settings.config_reset", "Settings reset"),
            (
                "settings.reload_hint",
                "Reload the app to apply all changes",
            ),
            ("settings.export_logs_title", "Export logs"),
            ("settings.confirm_delete_logs", "Delete all logs?"),
            (
                "settings.confirm_delete_logs_body",
                "This action cannot be undone.",
            ),
            ("settings.confirm_delete", "Delete"),
            ("settings.confirm_reset", "Reset settings?"),
            (
                "settings.confirm_reset_body",
                "All settings will return to defaults.",
            ),
            ("settings.confirm_reset_btn", "Reset"),
            // Sidebar
            ("sidebar.status", "Status"),
            ("sidebar.account", "Account"),
            ("sidebar.sync", "Sync"),
            ("sidebar.selective", "Folders"),
            ("sidebar.settings", "Settings"),
            ("sidebar.activity", "Activity"),
            // Widgets
            ("widget.cancel", "Cancel"),
            ("widget.confirm", "Confirm"),
        ])
    });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_key_translates_both_langs() {
        assert_eq!(t(Lang::Es, "status.idle"), "Listo");
        assert_eq!(t(Lang::En, "status.idle"), "Ready");
        assert_eq!(t(Lang::Es, "settings.title"), "Ajustes");
        assert_eq!(t(Lang::En, "settings.title"), "Settings");
    }

    #[test]
    fn missing_key_falls_back_to_key() {
        assert_eq!(t(Lang::Es, "no.such.key"), "no.such.key");
    }

    #[test]
    fn interpolation_replaces_placeholders() {
        assert_eq!(
            tf(
                Lang::Es,
                "account.storage_used",
                &[("used", "1 GB"), ("total", "15 GB")]
            ),
            "1 GB de 15 GB usados"
        );
    }

    #[test]
    fn language_code_roundtrip() {
        assert_eq!(code(from_code("es")), "es");
        assert_eq!(code(from_code("en")), "en");
        assert_eq!(code(from_code("xx")), "es");
    }

    #[test]
    fn languages_have_same_key_set() {
        for k in ES.keys() {
            assert!(EN.contains_key(k), "missing EN key: {k}");
        }
        for k in EN.keys() {
            assert!(ES.contains_key(k), "missing ES key: {k}");
        }
    }
}
