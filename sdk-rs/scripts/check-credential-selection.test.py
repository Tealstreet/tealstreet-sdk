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
        P = 'use safe_cex_engine::signing::Identity;\n'
        FIELD = 'pub struct Options { pub read_credentials: Option<Identity> }\n'
        USE = 'fn f(o: &Options) { o.read_credentials.as_ref(); }\n'
        adopted = {
            'imported': P + FIELD + USE,
            'pathed': 'pub struct Options { pub read_credentials: Option<safe_cex_engine::signing::Identity> }\n' + USE,
            'local_struct_elsewhere': P + FIELD + 'fn g() { struct Identity { id: u64 } }\n',
            'grouped_with_engine_glob': 'use safe_cex_engine::model::*;\n'
                                        'use safe_cex_engine::signing::{Identity, KeyRef};\n' + FIELD,
            'literal_pattern_param': P + FIELD +
                'fn a(read_credentials: Option<Identity>) -> Options { Options { read_credentials } }\n'
                'fn b(o: Options) { let Options { read_credentials, .. } = o; }\n'
                'fn c(x: Option<Identity>) -> Options { Options { read_credentials: x } }\n',
            'self_holder': P + 'pub struct Venue { read_credentials: Option<Identity> }\n'
                'impl Venue { fn f(&self) -> bool { self.read_credentials.is_some() } '
                'fn n() -> Self { Self { read_credentials: None } } }\n',
            'self_field_holder': P + FIELD + 'pub struct Venue { options: Options }\n'
                'impl Venue { fn f(&self) { self.options.read_credentials.as_ref(); } }\n',
            'rebind_borrow_move': P + FIELD + 'pub struct Venue { options: Options }\n'
                'fn n(options: Options) -> Venue { let mut options = options; helper(&options); '
                'options.read_credentials.take(); Venue { options } }\n',
            'impl_only_workspace_macro': P + FIELD + 'safe_cex_crypto::redacted_debug!(Options);\n'
                '#[derive(Clone, Debug, serde::Deserialize)]\n#[serde(default)]\npub struct Wire { a: u8 }\n'
                'fn g() { assert!(true); format!("x"); }\n' + USE,
        }
        counted = {
            'raw': 'pub struct Options { pub read_credentials: Option<ReadCredentials> }\n' + USE,
            'mixed': P + FIELD + 'pub struct B { pub read_credentials: Option<sign::Credentials> }\n',
            'unimported': FIELD,
            'import_in_other_fn': 'fn a() { use safe_cex_engine::signing::Identity; }\nstruct Identity;\n' + FIELD,
            'nested_module_local': P + 'mod m { struct Identity; pub struct O { pub read_credentials: Option<Identity> } }\n',
            'generic_parameter': P + 'pub struct O<Identity> { pub read_credentials: Option<Identity> }\n',
            'raw_identifier_holder': P + FIELD + 'pub struct B { pub r#read_credentials: Option<RawKeys> }\n',
            'non_option_holder': P + FIELD + 'pub struct B { pub read_credentials: RawKeys }\n',
            'aliased_import': 'use safe_cex_engine::signing::{Raw as Identity};\n' + FIELD,
            'renamed_local': 'use crate::Raw as Identity;\n' + FIELD,
            'type_alias': 'type Identity = RawKeys;\n' + FIELD,
            'glob_only': 'use other::*;\n' + FIELD,
            'callable': P + FIELD + 'fn read_credentials() -> RawKeys { RawKeys }\n',
            'local_binding': P + FIELD + 'fn g(raw: RawKeys) { let read_credentials = raw; }\n',
            'undeclared': USE,
            # Receivers must be typed to a proven holder.
            'foreign_receiver': P + FIELD + 'fn g(raw: &vendor::Options) { raw.read_credentials.as_ref(); }\n',
            'member_call': P + FIELD + 'fn g(raw: &Options) { raw.read_credentials(); }\n',
            'foreign_literal': P + FIELD + 'fn g() -> vendor::Options { vendor::Options { read_credentials: raw() } }\n',
            'foreign_pattern': P + FIELD + 'fn g(v: Raw) { let vendor::Options { read_credentials, .. } = v; }\n',
            'untyped_receiver': P + FIELD + 'fn g(o: Thing) { o.read_credentials.as_ref(); }\n',
            'rebound_receiver': P + FIELD + 'fn g(o: &Options, raw: Raw) { let o = raw; o.read_credentials.as_ref(); }\n',
            'closure_receiver': P + FIELD + 'fn g(o: &Options) { let h = |o: Raw| o.read_credentials; }\n',
            'pattern_receiver': P + FIELD + 'fn g(o: &Options, r: Raw) { if let Some(o) = r.get() { o.read_credentials.as_ref(); } }\n',
            'foreign_self': P + FIELD + 'impl vendor::Thing { fn g(&self) { self.read_credentials.as_ref(); } }\n',
            'unholder_self_field': P + FIELD + 'pub struct Venue { options: vendor::Options }\n'
                'impl Venue { fn g(&self) { self.options.read_credentials.as_ref(); } }\n',
            # Generators, roots and wrappers must be closed.
            'item_generator': P + 'install!();\n' + FIELD + USE,
            'block_generator': P + FIELD + 'fn g(o: &Options) { install!(); o.read_credentials.as_ref(); }\n',
            'untrusted_attribute': P + '#[vendor::magic]\n' + FIELD + USE,
            'untrusted_derive': P + '#[derive(Magic)]\npub struct W;\n' + FIELD + USE,
            'shadowed_trusted_macro': P + 'macro_rules! format { () => {} }\n' + FIELD + USE,
            'local_engine_module': 'mod safe_cex_engine { pub mod signing { pub struct Identity; } }\n'
                'pub struct Options { pub read_credentials: Option<safe_cex_engine::signing::Identity> }\n' + USE,
            'aliased_engine_root': 'use crate::wire as safe_cex_engine;\n'
                'pub struct Options { pub read_credentials: Option<safe_cex_engine::signing::Identity> }\n' + USE,
            'local_option': P + 'pub struct Option<T> { value: T, key: Vec<u8> }\n' + FIELD + USE,
            'foreign_holder_import': P + FIELD + 'mod m { use vendor::Options; fn g(o: &Options) { o.read_credentials.as_ref(); } }\n',
            'duplicate_holder_name': P + FIELD + 'mod m { pub struct Options { pub read_credentials: Option<RawKeys> } }\n' + USE,
            'unproven_glob': P + 'use vendor::*;\n' + FIELD + USE,
            'non_impl_workspace_macro': P + 'safe_cex_crypto::other_macro!(Options);\n' + FIELD + USE,
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
