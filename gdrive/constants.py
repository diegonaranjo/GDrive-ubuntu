"""Constantes centralizadas de la aplicación."""

import os
from pathlib import Path

# El daemon nativo (Rust, `gdrive-daemon`) es dueño del nombre de bus histórico;
# la UI GTK4 usa un ID propio para no colisionar.
DAEMON_ID = "com.gdrive.Client"
DAEMON_PATH = "/com/gdrive/Client"
DAEMON_INTERFACE = "com.gdrive.Daemon1"

APP_ID = "com.gdrive.ClientUI"
APP_NAME = "Google Drive"
APP_VERSION = "1.0.0"

RCLONE_REMOTE = os.environ.get("GDRIVE_REMOTE", "gdrive:")
SYNC_DIR_DEFAULT = str(Path.home() / "GoogleDrive")

COLORS = {
    "idle": "#1E8E3E",
    "syncing": "#1A73E8",
    "paused": "#F9AB00",
    "error": "#D93025",
}

STATUS_ICONS = {
    "idle": "gdrive-status-symbolic",
    "syncing": "gdrive-syncing-symbolic",
    "paused": "gdrive-paused-symbolic",
    "error": "gdrive-error-symbolic",
}


def get_status_texts():
    from .i18n import t
    return {
        "idle": t("status.idle"),
        "syncing": t("status.syncing"),
        "paused": t("status.paused"),
        "error": t("status.error"),
    }


def get_status_details():
    from .i18n import t
    return {
        "idle": t("status.detail.idle"),
        "syncing": t("status.detail.syncing"),
        "paused": t("status.detail.paused"),
        "error": t("status.detail.error"),
    }
