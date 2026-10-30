"""Loading of the external CSS."""

from pathlib import Path

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, GLib, Gtk

THEME_FILE = Path(__file__).resolve().parent.parent / "theme" / "theme.css"


def load_theme():
    """Loads theme/theme.css into the default display."""
    css = Gtk.CssProvider()
    try:
        css.load_from_path(str(THEME_FILE))
    except GLib.Error as e:
        print(f"[theme] No se pudo cargar {THEME_FILE}: {e}")
        return
    Gtk.StyleContext.add_provider_for_display(
        Gdk.Display.get_default(), css,
        Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
