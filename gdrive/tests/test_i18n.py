"""Tests for the bundled translation catalogs (no GTK, no D-Bus)."""

import unittest

from gdrive.i18n import _BUILTIN, t


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

    def test_missing_key_falls_back_to_the_key(self):
        self.assertEqual(t("no.existe"), "no.existe")


if __name__ == "__main__":
    unittest.main()
