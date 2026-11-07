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
    def test_engine_algorithm_needs_a_closed_crate(self):
        declaration = 'use safe_cex_engine::signing::{Algorithm, KeyRef};\nconst A: Algorithm = Algorithm::HmacSha256;\n'
        crates = {
            'clean': declaration + '#[derive(Debug)]\npub struct W;\nfn g() { assert!(true); }\n',
            'item_generator': 'install!();\n' + declaration,
            'block_generator': declaration + 'fn g() { install!(); let a = Algorithm::HmacSha256; }\n',
            'local_engine_module': 'mod safe_cex_engine { pub mod signing { pub enum Algorithm { HmacSha256 } } }\n' + declaration,
            'aliased_engine_root': 'use crate::wire as safe_cex_engine;\n' + declaration,
            'unproven_glob': 'use vendor::*;\n' + declaration,
            'untrusted_attribute': '#[vendor::magic]\n' + declaration,
            'renamed_engine_dependency': declaration,
            'path_module_outside_crate': '#[path = "../../opaque.rs"] mod opaque;\n' + declaration,
        }
        old_root = ban.ROOT
        with tempfile.TemporaryDirectory() as directory:
            try:
                ban.ROOT = pathlib.Path(directory)
                (ban.ROOT / 'Cargo.toml').write_text(ban.rust_scope.workspace_manifest())
                for crate, source in crates.items():
                    path = ban.ROOT / f'venues/{crate}/src/lib.rs'
                    path.parent.mkdir(parents=True)
                    path.write_text(source)
                    cargo = '' if crate == 'renamed_engine_dependency' else 'safe-cex-engine = { workspace = true }\n'
                    (ban.ROOT / f'venues/{crate}/Cargo.toml').write_text('[dependencies]\n' + cargo)
                self.assertEqual(ban.proven_crates(), {'clean'})
                found = ban.sites()
                self.assertNotIn('venues/clean/src/lib.rs', found)
                for crate in set(crates) - {'clean'}:
                    self.assertIn(f'venues/{crate}/src/lib.rs', found, crate)
            finally:
                ban.ROOT = old_root

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
