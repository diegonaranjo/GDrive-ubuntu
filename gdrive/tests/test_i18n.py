"""Tests for the bundled translation catalogs (no GTK, no D-Bus)."""

import unittest

from gdrive.i18n import _BUILTIN, set_language, t


class CatalogParityTest(unittest.TestCase):
    """`es` and `en` must expose exactly the same keys.

    A key present in one language only silently leaks the raw key to the user,
    which is how the Spanish-only runtime strings of issue #1 went unnoticed.
    """

    def test_both_languages_exist(self):
        self.assertEqual(sorted(_BUILTIN), ["en", "es"])

    def test_es_and_en_share_the_same_keys(self):
        es = set(_BUILTIN["es"])
        en = set(_BUILTIN["en"])
        self.assertEqual(sorted(es - en), [], "keys only in es")
        self.assertEqual(sorted(en - es), [], "keys only in en")

    def test_runtime_shell_keys_are_translated(self):
        # Keys added for the tray and the theme loader (issue #1). Asserting on
        # presence, not on the prose: the wording may change, the key may not.
        for key in ("tray.daemon_ensure_failed", "theme.load_failed"):
            self.assertIn(key, _BUILTIN["es"])
            self.assertIn(key, _BUILTIN["en"])

    def test_no_key_resolves_to_itself(self):
        for lang, catalog in _BUILTIN.items():
            for key, value in catalog.items():
                self.assertNotEqual(
                    value, key, f"{key} has no value in {lang}"
                )

    def test_es_runtime_values_are_not_the_english_text(self):
        """A new key copied from `en` into `es` is exactly the bug this guards.

        `value != key` cannot see it: an English sentence copied into the `es`
        catalog is a perfectly valid value, so the comparison has to be against
        the other language's value.
        """
        for key in ("tray.daemon_ensure_failed", "theme.load_failed"):
            self.assertNotEqual(
                _BUILTIN["es"][key],
                _BUILTIN["en"][key],
                f"{key} still carries the English text in es",
            )
        self.assertIn("No se pudo", _BUILTIN["es"]["tray.daemon_ensure_failed"])
        self.assertIn("cargar", _BUILTIN["es"]["theme.load_failed"])

    def test_missing_key_falls_back_to_the_key(self):
        self.assertEqual(t("no.existe"), "no.existe")


class RuntimeShellStringsTest(unittest.TestCase):
    """The tray and the theme loader resolve through the catalog, not literals."""

    def tearDown(self):
        set_language("es")

    def test_tray_message_in_both_languages(self):
        set_language("es")
        self.assertEqual(
            t("tray.daemon_ensure_failed"), "No se pudo asegurar el daemon"
        )
        set_language("en")
        self.assertEqual(
            t("tray.daemon_ensure_failed"), "Could not ensure the daemon"
        )

    def test_theme_message_interpolates_the_detail(self):
        set_language("es")
        self.assertEqual(
            t("theme.load_failed", path="/x/theme.css", error="boom"),
            "No se pudo cargar /x/theme.css: boom",
        )
        set_language("en")
        self.assertEqual(
            t("theme.load_failed", path="/x/theme.css", error="boom"),
            "Could not load /x/theme.css: boom",
        )

    def test_unsupported_language_falls_back_to_spanish(self):
        # A hand-edited settings.json must not turn the UI into raw key names.
        set_language("fr")
        self.assertEqual(
            t("tray.daemon_ensure_failed"), "No se pudo asegurar el daemon"
        )


if __name__ == "__main__":
    unittest.main()
