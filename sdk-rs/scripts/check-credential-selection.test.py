"""Adversarial controls for the shrinking credential inventory; no Rust build."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    'credential_ban', pathlib.Path(__file__).with_name('check-credential-selection.py'))
ban = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ban)


def counts(source, native_host=False):
    return dict(ban.rust_counts(source, native_host)[0])


def hidden(source):
    return ban.rust_counts(source)[1]


class CredentialSelectionBan(unittest.TestCase):
    def test_venue_selection_and_nonce_sites_are_counted_per_term(self):
        self.assertEqual(counts('let r = self.options.read_credentials.as_ref();'), {'read_credentials': 1})
        for site, term in [('cx.tag_account_read(id, slot, scope);', 'tag_account_read'),
                           ('nonce::ethereum_agent(key, now)', 'ethereum_agent'),
                           ('safe_cex_crypto::nonce::next("k", id, 1, 2)', 'nonce::next'),
                           ('static NONCES: Lock = Lock::new();', 'NONCES'),
                           ('let Options { read_credentials, .. } = options;', 'read_credentials'),
                           ('fn f(read: Option<ReadCredentials>) {}', 'ReadCredentials')]:
            self.assertEqual(counts(site), {term: 1}, site)

    def test_read_credentials_are_adopted_only_where_every_holder_is_proven(self):
        import tempfile
        old_root = ban.ROOT
        adopted = {
            'imported': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\nfn f(o: &Options) { o.read_credentials.as_ref(); }\n',
            'pathed': 'pub struct Options { pub read_credentials: Option<safe_cex_engine::signing::Identity> }\nfn f(o: &Options) { o.read_credentials.as_ref(); }\n',
            'local_struct_elsewhere': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\nfn g() { struct Identity { id: u64 } }\n',
            'grouped_with_glob': 'use wire::*;\nuse safe_cex_engine::signing::{Identity, KeyRef};\npub struct Options { pub read_credentials: Option<Identity> }\n',
            'literal_pattern_param': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\n'
                'fn a(read_credentials: Option<Identity>) -> Options { Options { read_credentials } }\n'
                'fn b(o: Options) { let Options { read_credentials, .. } = o; }\n'
                'fn c(x: Option<Identity>) -> Options { Options { read_credentials: x } }\n',
        }
        counted = {
            'raw': 'pub struct Options { pub read_credentials: Option<ReadCredentials> }\nfn f(o: &Options) { o.read_credentials.as_ref(); }\n',
            'mixed': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\npub struct B { pub read_credentials: Option<sign::Credentials> }\n',
            'unimported': 'pub struct Options { pub read_credentials: Option<Identity> }\n',
            'import_in_other_fn': 'fn a() { use safe_cex_engine::signing::Identity; }\n'
                                  'struct Identity;\npub struct Options { pub read_credentials: Option<Identity> }\n',
            'nested_module_local': 'use safe_cex_engine::signing::Identity;\nmod m { struct Identity; pub struct O { pub read_credentials: Option<Identity> } }\n',
            'generic_parameter': 'use safe_cex_engine::signing::Identity;\npub struct O<Identity> { pub read_credentials: Option<Identity> }\n',
            'raw_identifier_holder': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\npub struct B { pub r#read_credentials: Option<RawKeys> }\n',
            'non_option_holder': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\npub struct B { pub read_credentials: RawKeys }\n',
            'aliased_import': 'use safe_cex_engine::signing::{Raw as Identity};\npub struct Options { pub read_credentials: Option<Identity> }\n',
            'renamed_local': 'use crate::Raw as Identity;\npub struct Options { pub read_credentials: Option<Identity> }\n',
            'type_alias': 'type Identity = RawKeys;\npub struct Options { pub read_credentials: Option<Identity> }\n',
            'glob_only': 'use other::*;\npub struct Options { pub read_credentials: Option<Identity> }\n',
            'callable': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\nfn read_credentials() -> RawKeys { RawKeys }\n',
            'local_binding': 'use safe_cex_engine::signing::Identity;\npub struct Options { pub read_credentials: Option<Identity> }\nfn g(raw: RawKeys) { let read_credentials = raw; }\n',
            'undeclared': 'fn f(o: &Options) { o.read_credentials.as_ref(); }\n',
        }
        with tempfile.TemporaryDirectory() as directory:
            try:
                ban.ROOT = pathlib.Path(directory)
                for crate, source in {**adopted, **counted}.items():
                    path = ban.ROOT / f'venues/{crate}/src/lib.rs'
                    path.parent.mkdir(parents=True)
                    path.write_text(source)
                files = [(None, p, False) for p in sorted((ban.ROOT / 'venues').glob('*/src/**/*.rs'))]
                self.assertEqual(ban.identity_crates(files), set(adopted))
                (ban.ROOT / 'hosts/native/src').mkdir(parents=True)
                (ban.ROOT / ban.NATIVE_HOST).write_text('')
                ts = ban.ROOT / ban.TS_HOST
                ts.parent.mkdir(parents=True, exist_ok=True)
                ts.write_text('')
                result, _ = ban.sites()
                for crate in counted:
                    self.assertIn('read_credentials', result.get(f'venues/{crate}/src/lib.rs', {}), crate)
                for crate in adopted:
                    self.assertNotIn(f'venues/{crate}/src/lib.rs', result, crate)
                self.assertEqual(result['venues/raw/src/lib.rs'].get('ReadCredentials'), 1)
            finally:
                ban.ROOT = old_root

    def test_unrelated_neighbouring_edits_do_not_change_counts(self):
        before = 'let read = self.options.read_credentials.as_ref();'
        after = 'let read_key = self\n  .options /* c */ .read_credentials.as_ref();'
        self.assertEqual(counts(before), counts(after))

    def test_comments_literals_and_engine_selection_are_not_sites(self):
        source = '''// read_credentials
            let a = "tag_account_read";
            let (selection, key) = self.credentials.select(CredentialRole::Read { scope: None })?;
            selection.sign(cx, request, step);'''
        self.assertFalse(counts(source))

    def test_imports_and_renames_that_hide_sites_are_banned(self):
        for source in ['use safe_cex_crypto::nonce::next as n;', 'use safe_cex_crypto::nonce::{next};',
                       'use safe_cex_crypto::nonce::*;', 'use safe_cex_crypto::nonce as nc;',
                       'use safe_cex_engine::ReadCredentials as Rc;']:
            self.assertTrue(hidden(source), source)
        self.assertFalse(hidden('use safe_cex_engine::credential_selection::CredentialRole;'))

    def test_only_test_modules_are_excluded(self):
        source = ('#[cfg(test)]\nmod secondary_tests;\nfn live(r: ReadCredentials) {}\n'
                  '#[cfg(test)]\nmod tests {\n    fn t(r: ReadCredentials) { if x { } }\n}\n'
                  'impl Drop for V { fn drop(&mut self) { self.read_credentials.take(); } }\n')
        self.assertEqual(counts(source), {'ReadCredentials': 1, 'read_credentials': 1})
        with self.assertRaises(ValueError):
            counts('#[cfg(test)]\nmod tests {\n')

    def test_host_method_inference_counts_only_in_the_native_host(self):
        source = 'let read = request.method == safe_cex_engine::Method::Get;'
        self.assertFalse(counts(source))
        self.assertEqual(counts(source, native_host=True), {'Method::Get': 1})
        ts = "return readHttp && request.method.toUpperCase() === \"GET\" ? readHttp : http;"
        self.assertEqual(dict(ban.ts_counts(ts)), {'GET': 1})
        listed = "!['binance', 'binancepm', 'binanceinverse', 'newvenue', 'asterdex'].includes(exchange)"
        self.assertEqual(dict(ban.ts_counts(listed)), {'venue-list': 1})
        self.assertFalse(ban.ts_counts("if (route === 'secondary') return readHttp;"))

    def test_production_file_filter(self):
        for name, live in [('venues/x/src/backtests.rs', True), ('venues/x/src/tests.rs', False),
                           ('venues/x/src/secondary_tests.rs', False), ('venues/x/src/tests/a.rs', False)]:
            self.assertEqual(ban.production(pathlib.Path(name)), live, name)

    def test_inventory_may_only_shrink(self):
        site = {'read_credentials': 1}
        self.assertEqual(ban.check({'venues/x/src/lib.rs': site}, {'venues/x/src/lib.rs': site}, []), [])
        self.assertTrue(ban.check({'venues/x/src/lib.rs': {'read_credentials': 2}},
                                  {'venues/x/src/lib.rs': site}, []))
        grown = {'venues/x/src/lib.rs': {'read_credentials': 2}}
        self.assertTrue(ban.check(grown, grown, [('base', {'venues/x/src/lib.rs': site})]))
        self.assertEqual(ban.check({}, {}, [('base', {'venues/x/src/lib.rs': site})]), [])
        for bad in ['{"elsewhere/x.rs": {"read_credentials": 1}}', '{"venues/x/src/a.rs": {"other": 1}}']:
            with self.assertRaises(ValueError):
                ban.manifest(bad)


if __name__ == '__main__':
    unittest.main()
