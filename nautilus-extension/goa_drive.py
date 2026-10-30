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
        submenu = Nautilus.Menu()

        item_local = Nautilus.MenuItem(
            name="GoogleDriveExtension::open_local",
            label="Abrir carpeta local",
        )
        item_local.connect("activate", self._open_local)
        submenu.append_item(item_local)

        item_web = Nautilus.MenuItem(
            name="GoogleDriveExtension::open_web",
            label="Abrir en navegador",
        )
        item_web.connect("activate", self._open_web)
        submenu.append_item(item_web)

        item_sync = Nautilus.MenuItem(
            name="GoogleDriveExtension::sync_up",
            label="Sincronizar con rclone",
        )
        item_sync.connect("activate", self._sync_up)
        submenu.append_item(item_sync)

        top_menuitem = Nautilus.MenuItem(
            name="GoogleDriveExtension::parent_menu",
            label="Google Drive",
        )
        top_menuitem.set_submenu(submenu)

        return [top_menuitem]
