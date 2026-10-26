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
    def test_only_proven_engine_algorithm_variants_are_not_sites(self):
        def lines(source):
            declared = ban.declared_variants(source)
            return [number for number, line in enumerate(source.splitlines(), 1)
                    if not line.strip().startswith('//') and ban.counted(line, declared[number])]
        adopted = [
            'use safe_cex_engine::signing::{Algorithm, KeyRef};\nconst A: Algorithm = Algorithm::HmacSha256;',
            'fn f() {\n    use super::*;\n    use safe_cex_engine::signing::{Algorithm, KeyRef};\n'
            '    let a = Algorithm::Ed25519;\n}',
            'fn f() { let a = safe_cex_engine::signing::Algorithm::HmacSha256; }',
        ]
        for source in adopted:
            self.assertEqual(lines(source), [], source)
        counted = {
            'associated call': 'use safe_cex_engine::signing::Algorithm;\nlet m = Algorithm::hmac_sha256(key, msg);',
            'vendor path': 'let s = vendor::Algorithm::Ed25519(payload, key);',
            'local enum': 'enum Algorithm { HmacSha256 }\nlet a = Algorithm::HmacSha256;\nlet m = hmac(key);',
            'aliased': 'use safe_cex_engine::signing::Other as Algorithm;\nlet a = Algorithm::HmacSha256;',
            'glob only': 'use safe_cex_engine::signing::*;\nlet a = Algorithm::HmacSha256;',
            'import elsewhere': 'fn g() { use safe_cex_engine::signing::Algorithm; }\n'
                                'mod m { pub enum Algorithm { HmacSha256 } fn h() { let a = Algorithm::HmacSha256; } }',
            'same-line secret': 'use safe_cex_engine::signing::Algorithm;\n'
                                'let s = (Algorithm::HmacSha256, self.secret.clone());',
            'raw primitive': 'use safe_cex_engine::signing::Algorithm;\nlet mac = Hmac::<Sha256>::new_from_slice(key);',
        }
        for name, source in counted.items():
            self.assertNotEqual(lines(source), [], name)

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
                for family in ['asterdex', 'binance', 'bingx', 'bybit', 'phemex', 'bitrue']:
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
