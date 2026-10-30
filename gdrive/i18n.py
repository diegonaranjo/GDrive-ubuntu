"""Internationalization support for GDrive."""

import json
from pathlib import Path

_current_lang = "es"
_translations = {}


def set_language(lang):
    global _current_lang, _translations
    _current_lang = lang
    _translations = _load_translations(lang)


def get_language():
    return _current_lang


def _(key, **kwargs):
    text = _translations.get(key, key)
    if kwargs:
        text = text.format(**kwargs)
    return text


def t(key, **kwargs):
    return _(key, **kwargs)


def available_languages():
    return [("es", "Español"), ("en", "English")]


def _load_translations(lang):
    t = _BUILTIN.get(lang, {})
    # Try loading from external file (user overrides)
    custom = Path.home() / ".config" / "gdrive" / f"translations_{lang}.json"
    if custom.exists():
        try:
            t = {**t, **json.loads(custom.read_text())}
        except Exception:
            pass
    return t


_BUILTIN = {
    "es": {
        # ── Status page ──
        "status.title": "Estado",
        "status.idle": "Listo",
        "status.syncing": "Sincronizando…",
        "status.paused": "Pausado",
        "status.error": "Error",
        "status.detail.idle": "Google Drive está al día",
        "status.detail.syncing": "Sincronizando archivos con Google Drive",
        "status.detail.paused": "La sincronización está pausada",
        "status.detail.error": "Hubo un error durante la sincronización",
        "status.info_group": "Información",
        "status.last_sync": "Última sincronización",
        "status.last_sync_value": "Nunca",
        "status.files_synced": "Archivos sincronizados",
        "status.errors": "Errores",
        "status.speed": "Velocidad de transferencia",
        "status.sync_now": "Sincronizar ahora",
        "status.pause": "Pausar",
        "status.resume": "Reanudar",

        # ── Account page ──
        "account.title": "Cuenta",
        "account.not_signed_in": "Not signed in",
        "account.google_drive_account": "Google Drive account",
        "account.storage": "Almacenamiento",
        "account.storage_used": "{used} de {total} usados",
        "account.no_storage_info": "No se pudo obtener información",
        "account.no_limit": "Sin límite",
        "account.actions": "Acciones",
        "account.open_drive": "Abrir Google Drive",
        "account.manage_account": "Gestionar cuenta",
        "account.open": "Abrir",
        "account.refresh_info": "Actualizar información",

        # ── Sync settings page ──
        "sync.title": "Sincronización",
        "sync.location": "Ubicación",
        "sync.sync_folder": "Carpeta de sincronización",
        "sync.change": "Cambiar",
        "sync.mode": "Modo",
        "sync.file_mode": "Modo de archivos",
        "sync.file_mode_subtitle": "Cómo se almacenan los archivos",
        "sync.mode_mirror": "Espejo — Copia completa",
        "sync.mode_selective": "Puntual — Solo seleccionados",
        "sync.scheduling": "Programación",
        "sync.interval": "Intervalo",
        "sync.interval_subtitle": "Frecuencia de sincronización",
        "sync.interval_manual": "Manual",
        "sync.interval_5min": "5 minutos",
        "sync.interval_15min": "15 minutos",
        "sync.interval_30min": "30 minutos",
        "sync.interval_1hr": "1 hora",
        "sync.watch_changes": "Detección en tiempo real",
        "sync.watch_subtitle": "Sincronizar automáticamente al detectar cambios en la carpeta",
        "sync.pause_sync": "Pausar sincronización",
        "sync.pause_subtitle": "Detener sincronización automática",
        "sync.bandwidth": "Ancho de banda",
        "sync.global_limit": "Límite global",
        "sync.no_limit": "Vacío = sin límite",
        "sync.placeholder_bw": "Ej: 1M",
        "sync.advanced": "Avanzado",
        "sync.skip_hidden": "Omitir ocultos",
        "sync.store_metadata": "Guardar metadatos (rclone --metadata)",
        "sync.transfer_threads": "Hilos de transferencia",
        "sync.shortcuts": "Accesos",
        "sync.open_local": "Abrir carpeta local",
        "sync.open_browser": "Abrir en navegador",
        "sync.open": "Abrir",
        "sync.select_folder": "Seleccionar carpeta",
        "sync.config_saved": "Configuración guardada",

        # ── Selective sync page ──
        "selective.title": "Carpetas",
        "selective.refresh": "Actualizar",
        "selective.no_folders": "No hay carpetas",

        # ── Settings page ──
        "settings.title": "Ajustes",
        "settings.general": "General",
        "settings.notifications": "Notificaciones",
        "settings.startup": "Iniciar con el sistema",
        "settings.start_hidden": "Arrancar oculto, solo en la bandeja",
        "settings.nautilus": "Integrar en Nautilus",
        "settings.network": "Red",
        "settings.use_proxy": "Usar proxy",
        "settings.proxy_url": "URL del proxy",
        "settings.cache": "Caché",
        "settings.clear_cache": "Limpiar caché",
        "settings.cache_cleared": "Caché limpiada",
        "settings.security": "Seguridad",
        "settings.encryption": "Cifrado en reposo",
        "settings.logs": "Registros (Logs)",
        "settings.enable_logs": "Habilitar registros",
        "settings.enable_logs_subtitle": "Guardar actividad en archivos de log",
        "settings.log_level": "Nivel de registro",
        "settings.log_level_subtitle": "Información a registrar",
        "settings.log_level_error": "Solo errores",
        "settings.log_level_info": "Información",
        "settings.log_level_debug": "Depuración",
        "settings.log_level_trace": "Traza completa",
        "settings.log_rotate": "Rotación automática",
        "settings.log_rotate_subtitle": "Comprimir logs antiguos",
        "settings.log_max_size": "Tamaño máximo por archivo (MB)",
        "settings.log_max_files": "Máximo de archivos de log",
        "settings.log_files": "Archivos de registro",
        "settings.log_activity": "Actividad",
        "settings.log_rclone": "Rclone",
        "settings.log_debug": "Depuración",
        "settings.open": "Abrir",
        "settings.log_folder": "Carpeta de logs",
        "settings.open_folder": "Abrir carpeta",
        "settings.log_actions": "Acciones de registro",
        "settings.export_logs": "Exportar logs…",
        "settings.delete_all_logs": "Eliminar todos los logs",
        "settings.logs_exported": "Logs exportados",
        "settings.logs_deleted": "Logs eliminados",
        "settings.about": "Acerca de",
        "settings.version": "Versión",
        "settings.docs": "Documentación",
        "settings.reset_config": "Restablecer configuración",
        "settings.config_saved": "Configuración guardada",
        "settings.config_reset": "Configuración restablecida",
        "settings.reload_hint": "Recarga la app para aplicar todos los cambios",
        "settings.export_logs_title": "Exportar logs",

        # ── Settings dialogs ──
        "settings.confirm_delete_logs": "¿Eliminar todos los logs?",
        "settings.confirm_delete_logs_body": "Esta acción no se puede deshacer.",
        "settings.confirm_delete": "Eliminar",
        "settings.confirm_reset": "¿Restablecer configuración?",
        "settings.confirm_reset_body": "Volverás a los valores por defecto.",
        "settings.confirm_reset_btn": "Restablecer",

        # ── Activity page ──
        "activity.title": "Actividad",
        "activity.clear": "Limpiar",
        "activity.search_placeholder": "Buscar en actividad…",
        "activity.no_activity": "Sin actividad",

        # ── Window / sidebar ──
        "sidebar.status": "Estado",
        "sidebar.account": "Cuenta",
        "sidebar.sync": "Sincronización",
        "sidebar.selective": "Carpetas",
        "sidebar.settings": "Ajustes",
        "sidebar.activity": "Actividad",

        # ── Window sign-in dialog ──
        "signin.title": "Sign in to Google Drive",
        "signin.body": ("No Google Drive account is linked yet.\n\n"
                        'Click "Sign in" to open your browser and authorize this app. '
                        "No terminal needed."),
        "signin.confirm": "Sign in",
        "signin.opening_browser": "Opening browser for sign-in…",
        "signin.success": "Signed in successfully",
        "signin.failed": "Sign-in failed — try again from the tray menu",

        # ── Window notifications ──
        "sync.finished_ok": "Sincronización completada",
        "sync.finished_error": "Error de sincronización",

        # ── Widgets ──
        "widget.cancel": "Cancelar",
        "widget.confirm": "Confirmar",

        # ── Tray ──
        "tray.idle": "Listo",
        "tray.syncing": "Sincronizando…",
        "tray.paused": "Pausado",
        "tray.error": "Error",
        "tray.session_started": "Sesión iniciada",
        "tray.settings": "Configuración",
        "tray.sync_now": "Sincronizar ahora",
        "tray.pause": "Pausar",
        "tray.resume": "Reanudar",
        "tray.open_folder": "Abrir carpeta local",
        "tray.open_browser": "Abrir en navegador",
        "tray.manage_account": "Gestionar cuenta",
        "tray.sign_in": "Iniciar sesión en Google",
        "tray.sign_out": "Cerrar sesión",
        "tray.quit": "Salir",
        "tray.notify_sync_started": "Sincronización iniciada",
        "tray.notify_opening_browser": "Abriendo el navegador para iniciar sesión…",
        "tray.notify_signin_ok": "Sesión iniciada correctamente",
        "tray.notify_signin_fail": "No se pudo iniciar sesión",
        "tray.notify_signout_ok": "Sesión cerrada",
        "tray.notify_signout_fail": "No se pudo cerrar sesión",
        "tray.sync_started_from_tray": "Sincronización iniciada desde tray",
        "tray.opening_settings": "Abriendo configuración",
        "tray.opening_folder": "Abriendo carpeta: {path}",
        "tray.opening_web": "Abriendo Google Drive en navegador",
        "tray.opening_manage": "Abriendo gestión de cuenta Google",
        "tray.quitting": "Saliendo de Google Drive tray",
        "tray.sync_paused": "Sincronización pausada",
        "tray.sync_resumed": "Sincronización reanudada",

        # ── Sync manager ──
        "sync_manager.starting": "Iniciando sincronización",
        "sync_manager.completed": "Sincronización completada",
        "sync_manager.failed": "Error en sincronización",
        "sync_manager.paused": "Pausado",
        "sync_manager.resumed": "Reanudado",

        # ── Singleton / single instance ──
        "app.already_running_title": "Google Drive ya está abierto",
        "app.already_running_body": "Ya hay una instancia de Google Drive en ejecución. Se ha activado la ventana existente.",
        "app.already_running_notif": "Google Drive ya está abierto",

        # ── Language selector ──
        "settings.language": "Idioma",
        "settings.language_subtitle": "Idioma de la interfaz",
    },
    "en": {
        # ── Status page ──
        "status.title": "Status",
        "status.idle": "Ready",
        "status.syncing": "Syncing…",
        "status.paused": "Paused",
        "status.error": "Error",
        "status.detail.idle": "Google Drive is up to date",
        "status.detail.syncing": "Syncing files with Google Drive",
        "status.detail.paused": "Sync is paused",
        "status.detail.error": "An error occurred during sync",
        "status.info_group": "Information",
        "status.last_sync": "Last sync",
        "status.last_sync_value": "Never",
        "status.files_synced": "Files synced",
        "status.errors": "Errors",
        "status.speed": "Transfer speed",
        "status.sync_now": "Sync now",
        "status.pause": "Pause",
        "status.resume": "Resume",

        # ── Account page ──
        "account.title": "Account",
        "account.not_signed_in": "Not signed in",
        "account.google_drive_account": "Google Drive account",
        "account.storage": "Storage",
        "account.storage_used": "{used} of {total} used",
        "account.no_storage_info": "Could not retrieve information",
        "account.no_limit": "No limit",
        "account.actions": "Actions",
        "account.open_drive": "Open Google Drive",
        "account.manage_account": "Manage account",
        "account.open": "Open",
        "account.refresh_info": "Refresh information",

        # ── Sync settings page ──
        "sync.title": "Sync",
        "sync.location": "Location",
        "sync.sync_folder": "Sync folder",
        "sync.change": "Change",
        "sync.mode": "Mode",
        "sync.file_mode": "File mode",
        "sync.file_mode_subtitle": "How files are stored",
        "sync.mode_mirror": "Mirror — Full copy",
        "sync.mode_selective": "Selective — Selected only",
        "sync.scheduling": "Scheduling",
        "sync.interval": "Interval",
        "sync.interval_subtitle": "Sync frequency",
        "sync.interval_manual": "Manual",
        "sync.interval_5min": "5 minutes",
        "sync.interval_15min": "15 minutes",
        "sync.interval_30min": "30 minutes",
        "sync.interval_1hr": "1 hour",
        "sync.watch_changes": "Real-time detection",
        "sync.watch_subtitle": "Sync automatically when changes are detected in the folder",
        "sync.pause_sync": "Pause sync",
        "sync.pause_subtitle": "Stop automatic sync",
        "sync.bandwidth": "Bandwidth",
        "sync.global_limit": "Global limit",
        "sync.no_limit": "Empty = no limit",
        "sync.placeholder_bw": "e.g. 1M",
        "sync.advanced": "Advanced",
        "sync.skip_hidden": "Skip hidden files",
        "sync.store_metadata": "Store metadata (rclone --metadata)",
        "sync.transfer_threads": "Transfer threads",
        "sync.shortcuts": "Shortcuts",
        "sync.open_local": "Open local folder",
        "sync.open_browser": "Open in browser",
        "sync.open": "Open",
        "sync.select_folder": "Select folder",
        "sync.config_saved": "Settings saved",

        # ── Selective sync page ──
        "selective.title": "Folders",
        "selective.refresh": "Refresh",
        "selective.no_folders": "No folders",

        # ── Settings page ──
        "settings.title": "Settings",
        "settings.general": "General",
        "settings.notifications": "Notifications",
        "settings.startup": "Start on system login",
        "settings.start_hidden": "Start hidden, tray only",
        "settings.nautilus": "Integrate in Nautilus",
        "settings.network": "Network",
        "settings.use_proxy": "Use proxy",
        "settings.proxy_url": "Proxy URL",
        "settings.cache": "Cache",
        "settings.clear_cache": "Clear cache",
        "settings.cache_cleared": "Cache cleared",
        "settings.security": "Security",
        "settings.encryption": "Encryption at rest",
        "settings.logs": "Logs",
        "settings.enable_logs": "Enable logging",
        "settings.enable_logs_subtitle": "Save activity to log files",
        "settings.log_level": "Log level",
        "settings.log_level_subtitle": "Information to log",
        "settings.log_level_error": "Errors only",
        "settings.log_level_info": "Information",
        "settings.log_level_debug": "Debug",
        "settings.log_level_trace": "Full trace",
        "settings.log_rotate": "Auto rotation",
        "settings.log_rotate_subtitle": "Compress old logs",
        "settings.log_max_size": "Max file size (MB)",
        "settings.log_max_files": "Max log files",
        "settings.log_files": "Log files",
        "settings.log_activity": "Activity",
        "settings.log_rclone": "Rclone",
        "settings.log_debug": "Debug",
        "settings.open": "Open",
        "settings.log_folder": "Log folder",
        "settings.open_folder": "Open folder",
        "settings.log_actions": "Log actions",
        "settings.export_logs": "Export logs…",
        "settings.delete_all_logs": "Delete all logs",
        "settings.logs_exported": "Logs exported",
        "settings.logs_deleted": "Logs deleted",
        "settings.about": "About",
        "settings.version": "Version",
        "settings.docs": "Documentation",
        "settings.reset_config": "Reset settings",
        "settings.config_saved": "Settings saved",
        "settings.config_reset": "Settings reset",
        "settings.reload_hint": "Reload the app to apply all changes",
        "settings.export_logs_title": "Export logs",

        # ── Settings dialogs ──
        "settings.confirm_delete_logs": "Delete all logs?",
        "settings.confirm_delete_logs_body": "This action cannot be undone.",
        "settings.confirm_delete": "Delete",
        "settings.confirm_reset": "Reset settings?",
        "settings.confirm_reset_body": "All settings will return to defaults.",
        "settings.confirm_reset_btn": "Reset",

        # ── Activity page ──
        "activity.title": "Activity",
        "activity.clear": "Clear",
        "activity.search_placeholder": "Search activity…",
        "activity.no_activity": "No activity",

        # ── Window / sidebar ──
        "sidebar.status": "Status",
        "sidebar.account": "Account",
        "sidebar.sync": "Sync",
        "sidebar.selective": "Folders",
        "sidebar.settings": "Settings",
        "sidebar.activity": "Activity",

        # ── Window sign-in dialog ──
        "signin.title": "Sign in to Google Drive",
        "signin.body": ("No Google Drive account is linked yet.\n\n"
                        'Click "Sign in" to open your browser and authorize this app. '
                        "No terminal needed."),
        "signin.confirm": "Sign in",
        "signin.opening_browser": "Opening browser for sign-in…",
        "signin.success": "Signed in successfully",
        "signin.failed": "Sign-in failed — try again from the tray menu",

        # ── Window notifications ──
        "sync.finished_ok": "Sync completed",
        "sync.finished_error": "Sync error",

        # ── Widgets ──
        "widget.cancel": "Cancel",
        "widget.confirm": "Confirm",

        # ── Tray ──
        "tray.idle": "Ready",
        "tray.syncing": "Syncing…",
        "tray.paused": "Paused",
        "tray.error": "Error",
        "tray.session_started": "Signed in",
        "tray.settings": "Settings",
        "tray.sync_now": "Sync now",
        "tray.pause": "Pause",
        "tray.resume": "Resume",
        "tray.open_folder": "Open local folder",
        "tray.open_browser": "Open in browser",
        "tray.manage_account": "Manage account",
        "tray.sign_in": "Sign in to Google",
        "tray.sign_out": "Sign out",
        "tray.quit": "Quit",
        "tray.notify_sync_started": "Sync started",
        "tray.notify_opening_browser": "Opening browser to sign in…",
        "tray.notify_signin_ok": "Signed in successfully",
        "tray.notify_signin_fail": "Sign-in failed",
        "tray.notify_signout_ok": "Signed out",
        "tray.notify_signout_fail": "Sign-out failed",
        "tray.sync_started_from_tray": "Sync started from tray",
        "tray.opening_settings": "Opening settings",
        "tray.opening_folder": "Opening folder: {path}",
        "tray.opening_web": "Opening Google Drive in browser",
        "tray.opening_manage": "Opening Google account management",
        "tray.quitting": "Quitting Google Drive tray",
        "tray.sync_paused": "Sync paused",
        "tray.sync_resumed": "Sync resumed",

        # ── Sync manager ──
        "sync_manager.starting": "Starting sync",
        "sync_manager.completed": "Sync completed",
        "sync_manager.failed": "Sync error",
        "sync_manager.paused": "Paused",
        "sync_manager.resumed": "Resumed",

        # ── Singleton / single instance ──
        "app.already_running_title": "Google Drive is already open",
        "app.already_running_body": "An instance of Google Drive is already running. The existing window has been activated.",
        "app.already_running_notif": "Google Drive is already open",

        # ── Language selector ──
        "settings.language": "Language",
        "settings.language_subtitle": "Interface language",
    },
}
