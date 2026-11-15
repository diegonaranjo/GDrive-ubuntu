"""Nautilus extension labels (issue #1, S3): inline ES/EN map, no gdrive import."""

import sys
import unittest
from pathlib import Path


class NautilusLabelsTest(unittest.TestCase):
    """The extension keeps an inline ES/EN map and never imports `gdrive`.

    `gi` is stubbed: the real module needs a running Nautilus session, and the
    extension only touches it at class-definition time.
    """

    ES = {
        "open_local": "Abrir carpeta local",
        "open_web": "Abrir en navegador",
        "sync_up": "Sincronizar con rclone",
        "parent": "Google Drive",
    }

    @classmethod
    def setUpClass(cls):
        import importlib.util
        import types

        gi = types.ModuleType("gi")
        gi.require_version = lambda *a, **k: None
        repo = types.ModuleType("gi.repository")
        repo.Nautilus = types.SimpleNamespace(
            Menu=object,
            MenuItem=object,
            FileInfo=object,
            MenuProvider=type("MenuProvider", (), {"__init__": lambda self: None}),
        )
        repo.GObject = types.SimpleNamespace(
            GObject=type("GObject", (), {"__init__": lambda self: None})
        )
        gi.repository = repo
        cls._saved = {k: sys.modules.get(k) for k in ("gi", "gi.repository")}
        sys.modules["gi"] = gi
        sys.modules["gi.repository"] = repo

        src = (
            Path(__file__).resolve().parents[2]
            / "nautilus-extension"
            / "goa_drive.py"
        )
        spec = importlib.util.spec_from_file_location("goa_drive_under_test", src)
        cls.mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.mod)
        cls.src_text = src.read_text()

    @classmethod
    def tearDownClass(cls):
        for key, value in cls._saved.items():
            if value is None:
                sys.modules.pop(key, None)
            else:
                sys.modules[key] = value

    def tearDown(self):
        self.mod.CONFIG_FILE = self.real_config

    def setUp(self):
        import tempfile

        self.real_config = self.mod.CONFIG_FILE
        self.tmp = Path(tempfile.mkdtemp())

    def _configure(self, payload):
        import json

        path = self.tmp / "settings.json"
        if payload is not None:
            path.write_text(
                payload if isinstance(payload, str) else json.dumps(payload)
            )
        self.mod.CONFIG_FILE = path

    def test_extension_does_not_import_the_gdrive_package(self):
        self.assertNotIn("import gdrive", self.src_text)
        self.assertNotIn("from gdrive", self.src_text)

    def test_default_language_is_spanish(self):
        self._configure(None)
        self.assertEqual(self.mod._labels(), self.ES)

    def test_english_settings_switch_the_labels(self):
        self._configure({"language": "en"})
        self.assertEqual(self.mod._labels()["open_local"], "Open local folder")

    def test_unknown_language_falls_back_to_spanish(self):
        self._configure({"language": "fr"})
        self.assertEqual(self.mod._labels(), self.ES)

    def test_malformed_settings_fall_back_to_spanish(self):
        self._configure("{not json")
        self.assertEqual(self.mod._labels(), self.ES)


if __name__ == "__main__":
    unittest.main()
