"""Adversarial controls for the shrinking credential inventory; no Rust build."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    'credential_ban', pathlib.Path(__file__).with_name('check-credential-selection.py'))
ban = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ban)


class CredentialSelectionBan(unittest.TestCase):
    def test_venue_selection_and_nonce_sites_are_frozen_regardless_of_formatting(self):
        source = 'let c = self.options.read_credentials.as_ref().filter(|_| method == Method::Get);'
        wrapped = 'let c = self . options\n . read_credentials /* c */ .as_ref().filter(|_| method == Method::Get);'
        self.assertEqual(ban.rust_fingerprints(source), ban.rust_fingerprints(wrapped))
        for site in ['cx.tag_account_read(id, slot, scope);', 'nonce::ethereum_agent(key, now)',
                     'safe_cex_crypto::nonce::next("k", id, 1, 2)', 'static NONCES: Lock = Lock::new();',
                     'fn f(read: Option<ReadCredentials>) {}']:
            self.assertTrue(ban.rust_fingerprints(site), site)

    def test_comments_literals_and_engine_selection_are_not_sites(self):
        source = '''// read_credentials
            let a = "tag_account_read";
            let (selection, key) = self.credentials.select(CredentialRole::Read { scope: None })?;
            selection.sign(cx, request, step);'''
        self.assertFalse(ban.rust_fingerprints(source))

    def test_host_method_inference_counts_only_in_the_native_host(self):
        source = 'let read = request.method == safe_cex_engine::Method::Get;'
        self.assertFalse(ban.rust_fingerprints(source))
        self.assertTrue(ban.rust_fingerprints(source, native_host=True))
        ts = "return readHttp && request.method.toUpperCase() === 'GET' ? readHttp : http;"
        self.assertTrue(ban.ts_fingerprints(ts))
        self.assertFalse(ban.ts_fingerprints("if (route === 'secondary') return readHttp;"))

    def test_inline_test_modules_are_excluded(self):
        source = 'fn live() {}\n#[cfg(test)]\nmod tests {\n    fn t(r: ReadCredentials) {}\n}\n'
        self.assertFalse(ban.rust_fingerprints(ban.without_test_module(source)))

    def test_inventory_may_only_shrink(self):
        site = {'a' * 64: 1}
        self.assertEqual(ban.check({'venues/x/src/lib.rs': site}, {'venues/x/src/lib.rs': site}, []), [])
        self.assertTrue(ban.check({'venues/x/src/lib.rs': {'a' * 64: 2}}, {'venues/x/src/lib.rs': site}, []))
        grown = {'venues/x/src/lib.rs': {'a' * 64: 1, 'b' * 64: 1}}
        self.assertTrue(ban.check(grown, grown, [('base', {'venues/x/src/lib.rs': site})]))
        self.assertEqual(ban.check({}, {}, [('base', {'venues/x/src/lib.rs': site})]), [])
        with self.assertRaises(ValueError):
            ban.manifest('{"elsewhere/x.rs": {"%s": 1}}' % ('a' * 64))


if __name__ == '__main__':
    unittest.main()
