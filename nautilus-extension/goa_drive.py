import json
import os
import subprocess
from pathlib import Path
from gi.repository import Nautilus, GObject
from typing import List

CONFIG_DIR = Path.home() / ".config" / "gdrive"
CONFIG_FILE = CONFIG_DIR / "settings.json"
SYNC_DIR_DEFAULT = str(Path.home() / "GoogleDrive")


def _sync_dir():
    """Reads sync_dir from the app settings, with a fallback to ~/GoogleDrive."""
    try:
        if CONFIG_FILE.exists():
            cfg = json.loads(CONFIG_FILE.read_text())
            return cfg.get("sync_dir", SYNC_DIR_DEFAULT)
    except Exception:
        pass
    return SYNC_DIR_DEFAULT


# The labels are kept inline in both languages on purpose: this file is copied
# alone into the Nautilus extensions directory, so importing the `gdrive`
# package would fail there. Loading an extra Python module into the Nautilus
# process is also what triggered the dual-libpython crash documented in
# AGENTS.md (Known issues).
_LABELS = {
    "es": {
        "open_local": "Abrir carpeta local",
        "open_web": "Abrir en navegador",
        "sync_up": "Sincronizar con rclone",
        "parent": "Google Drive",
    },
    "en": {
        "open_local": "Open local folder",
        "open_web": "Open in browser",
        "sync_up": "Sync with rclone",
        "parent": "Google Drive",
    },
}


def _labels():
    """Menu labels for the configured language, defaulting to Spanish."""
    lang = "es"
    try:
        if CONFIG_FILE.exists():
            lang = json.loads(CONFIG_FILE.read_text()).get("language", "es")
    except Exception:
        pass
    return _LABELS.get(lang, _LABELS["es"])


class GoogleDriveExtension(GObject.GObject, Nautilus.MenuProvider):
    def _open_local(self, *args):
        subprocess.Popen(["xdg-open", _sync_dir()])

    def _sync_up(self, *args):
        subprocess.Popen(["gnome-terminal", "--", "rclone", "bisync",
                         "gdrive:/", _sync_dir(), "--resync"])

    def _open_web(self, *args):
        subprocess.Popen(["xdg-open", "https://drive.google.com"])

    def get_file_items(self, files: List[Nautilus.FileInfo]) -> List[Nautilus.MenuItem]:
        return []

    def get_background_items(self, current_folder: Nautilus.FileInfo) -> List[Nautilus.MenuItem]:
        labels = _labels()
        submenu = Nautilus.Menu()

        item_local = Nautilus.MenuItem(
            name="GoogleDriveExtension::open_local",
            label=labels["open_local"],
        )
        item_local.connect("activate", self._open_local)
        submenu.append_item(item_local)

        item_web = Nautilus.MenuItem(
            name="GoogleDriveExtension::open_web",
            label=labels["open_web"],
        )
        item_web.connect("activate", self._open_web)
        submenu.append_item(item_web)

        item_sync = Nautilus.MenuItem(
            name="GoogleDriveExtension::sync_up",
            label=labels["sync_up"],
        )
        item_sync.connect("activate", self._sync_up)
        submenu.append_item(item_sync)

        top_menuitem = Nautilus.MenuItem(
            name="GoogleDriveExtension::parent_menu",
            label=labels["parent"],
        )
        top_menuitem.set_submenu(submenu)

        return [top_menuitem]
