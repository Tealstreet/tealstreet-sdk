"""Negative controls for the existing CI ban (no build or credentials)."""
import contextlib
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('signing_ban', pathlib.Path(__file__).with_name('check-signing.py'))
ban = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ban)


class SigningBan(unittest.TestCase):
    def test_new_or_duplicated_secret_holder_fails_but_removal_is_allowed(self):
        old_root = ban.ROOT
        with tempfile.TemporaryDirectory() as directory:
            try:
                ban.ROOT = pathlib.Path(directory)
                source = ban.ROOT / 'venues/reference/src/lib.rs'
                source.parent.mkdir(parents=True)
                source.write_text('fn legacy_secret() {}\n')
                baseline = ban.ROOT / 'scripts/signing-legacy-sites.json'
                baseline.parent.mkdir()
                baseline.write_text(json.dumps(ban.sites()))
                self.assertFalse(ban.main())
                for body in ['fn legacy_secret() {}\n' * 2, 'fn new_secret() {}\n']:
                    source.write_text(body)
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertTrue(ban.main())
                source.write_text('fn uses_provider_refs() {}\n')
                self.assertFalse(ban.main())
                for family in ['asterdex', 'binance', 'bingx', 'bybit']:
                    migrated = ban.ROOT / f'venues/{family}/src/sign.rs'
                    migrated.parent.mkdir(parents=True)
                    for body in ['fn new_secret() {}', 'fn hmac_sha256() {}']:
                        migrated.write_text(body)
                        with contextlib.redirect_stdout(io.StringIO()):
                            self.assertTrue(ban.main())
                    migrated.write_text('fn uses_provider_refs() {}')
            finally:
                ban.ROOT = old_root


if __name__ == '__main__':
    unittest.main()
