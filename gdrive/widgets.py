"""Widgets reutilizables GTK4/libadwaita."""

from .i18n import t

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk


# Dialogos abiertos a la espera de respuesta. `Adw.MessageDialog` no se registra
# como ventana de la aplicacion, y el patron de uso (`dlg = ConfirmDialog(...)`,
# `dlg.present()`) deja la unica referencia en una variable local del callback:
# PyGObject destruia el dialogo mientras todavia estaba en pantalla. Medido con
# una referencia debil: a los 2 s el objeto Python ya estaba muerto.
# `destroy` se emite al responder, asi que la salida siempre se libera.
_OPEN_DIALOGS = set()


class SidebarButton(Gtk.Button):
    def __init__(self, label, icon_name, tag):
        super().__init__(css_classes=["sidebar-item", "flat"])
        self.tag = tag
        self.set_halign(Gtk.Align.FILL)

        box = Gtk.Box(spacing=12, margin_start=8, margin_end=8,
                      margin_top=4, margin_bottom=4)
        self.set_child(box)

        self._img = Gtk.Image.new_from_icon_name(icon_name)
        self._img.set_pixel_size(20)
        box.append(self._img)

        self._lbl = Gtk.Label(label=label, hexpand=True, xalign=0)
        box.append(self._lbl)

        self._badge = Gtk.Box(css_classes=["status-dot"])
        self._badge.set_size_request(8, 8)
        # Sin valign el Box se estira a la altura de la fila (.sidebar-item
        # pide min-height 36px) y el border-radius de 9999px lo dibuja como
        # una pastilla vertical en lugar de un punto.
        self._badge.set_valign(Gtk.Align.CENTER)
        self._badge.set_halign(Gtk.Align.END)
        self._badge.set_visible(False)
        self._badge_css = Gtk.CssProvider()
        self._badge.get_style_context().add_provider(
            self._badge_css, Gtk.STYLE_PROVIDER_PRIORITY_USER)
        box.append(self._badge)

    def set_badge_color(self, color_hex):
        if color_hex is None:
            self._badge.set_visible(False)
            return
        # Un solo provider por boton: crear uno por cambio de estado los
        # acumula en el widget durante toda la sesion.
        self._badge_css.load_from_data(
            f".status-dot {{ background: {color_hex}; }}".encode())
        self._badge.set_visible(True)


STATUS_BADGE_COLORS = {
    "idle": "#1E8E3E",
    "syncing": "#1A73E8",
    "paused": "#F9AB00",
    "error": "#D93025",
}


class StatusBadge(Gtk.Box):
    def __init__(self):
        super().__init__(halign=Gtk.Align.CENTER, margin_top=10)
        self._icon = Gtk.Image(pixel_size=48)
        self._icon.add_css_class("status-badge-icon")
        self.append(self._icon)
        self.set_status("idle")

    def set_status(self, status):
        color = STATUS_BADGE_COLORS.get(status, "#888888")
        self.add_css_class("status-badge")
        for s in ("idle", "syncing", "paused", "error"):
            self.remove_css_class(s)
        self.add_css_class(status)
        p = Gtk.CssProvider()
        p.load_from_data(
            f".status-badge {{ color: {color}; }}".encode())
        self.get_style_context().add_provider(p, Gtk.STYLE_PROVIDER_PRIORITY_USER)
        self._icon.set_from_icon_name(
            {"idle": "gdrive-status-symbolic",
             "syncing": "gdrive-syncing-symbolic",
             "paused": "gdrive-paused-symbolic",
             "error": "gdrive-error-symbolic"}.get(status, "dialog-information-symbolic"))


class ConfirmDialog(Adw.MessageDialog):
    def __init__(self, parent, title, body, confirm_label=None,
                 destructive=True):
        if confirm_label is None:
            confirm_label = t("widget.confirm")
        super().__init__(transient_for=parent, modal=True,
                         heading=title, body=body)
        self.add_response("cancel", t("widget.cancel"))
        self.add_response("confirm", confirm_label)
        if destructive:
            self.set_response_appearance("confirm", Adw.ResponseAppearance.DESTRUCTIVE)
        else:
            self.set_response_appearance("confirm", Adw.ResponseAppearance.SUGGESTED)
        self.set_default_response("cancel")
        self.set_close_response("cancel")
        _OPEN_DIALOGS.add(self)
        self.connect("destroy", lambda d: _OPEN_DIALOGS.discard(d))
