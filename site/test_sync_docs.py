#!/usr/bin/env python3
"""Regression tests for the site version's Cargo.toml source of truth."""

import importlib.util
import pathlib
import re
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
VERSION_RE = re.compile(r'^\s*version\s*=\s*"([^"]+)"', re.M)


def package_version(text: str) -> str:
    match = VERSION_RE.search(text)
    if not match:
        raise AssertionError("Cargo.toml: missing package version")
    return match.group(1)


def load_sync_docs():
    spec = importlib.util.spec_from_file_location(
        "sync_docs", ROOT / "site" / "sync-docs.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class SiteVersionTest(unittest.TestCase):
    def test_sync_uses_cargo_package_version(self):
        cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        expected = package_version(cargo_toml)

        with tempfile.TemporaryDirectory() as directory:
            temporary_root = pathlib.Path(directory)
            (temporary_root / "Cargo.toml").write_text(cargo_toml, encoding="utf-8")
            hugo_toml = temporary_root / "hugo.toml"
            hugo_toml.write_text('[params]\n  version = "stale"\n', encoding="utf-8")

            sync_docs = load_sync_docs()
            sync_docs.ROOT = temporary_root
            sync_docs.HUGO_TOML = hugo_toml
            sync_docs.sync_version()

            actual = package_version(hugo_toml.read_text(encoding="utf-8"))
            self.assertEqual(actual, expected)

    def test_templates_read_the_site_version_parameter(self):
        homepage = (ROOT / "site/layouts/index.html").read_text(encoding="utf-8")
        seo = (ROOT / "site/layouts/partials/seo.html").read_text(encoding="utf-8")

        self.assertIn(
            'class="release-chip" aria-hidden="true">v{{ site.Params.version }} · preview',
            homepage,
        )
        self.assertIsNone(re.search(r'class="release-chip"[^>]*>v?0\.1\.0', homepage))
        self.assertNotIn('· caught', homepage)
        self.assertIn('"softwareVersion" site.Params.version', seo)
        self.assertIsNone(re.search(r'"softwareVersion"\s+"?0\.1\.0', seo))


if __name__ == "__main__":
    unittest.main()
