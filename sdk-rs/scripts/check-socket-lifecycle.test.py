"""Adversarial controls for the temporary shared socket migration ratchet."""
import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location(
    'socket_ban', pathlib.Path(__file__).with_name('check-socket-lifecycle.py'))
ban = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ban)


class SocketLifecycleBan(unittest.TestCase):
    def test_all_receivers_qualified_references_and_raw_names_are_counted(self):
        for source in ('cx.connect(url);', 'alias.connect(url);', 'Alias::connect(cx, url);',
                       '<Cx as Trait>::connect(cx, url);', 'cx.r#connect_signed(url, step);',
                       'let method = Cx::connect;', '(alias).connect::<T>(url);'):
            with self.subTest(source=source):
                self.assertEqual(sum(ban.lex.fingerprints(source).values()), 1)
                self.assertEqual(sum(ban.lex.fingerprints(source + source).values()), 2)

    def test_comments_literals_and_managed_wrappers_are_not_legacy_calls(self):
        self.assertFalse(ban.lex.fingerprints('// cx.connect(x);\nself.ensure_feed(cx, feed);'))
        self.assertFalse(ban.lex.fingerprints('let text = r###"cx.connect(url)"###;'))

    def test_macro_indirection_and_includes_cannot_escape(self):
        self.assertTrue(ban.lex.fingerprints('call!(cx, connect);'))
        for source in ('include!("other.rs");',
                       'macro_rules! m { ($c:expr) => { $c.connect(url) } }',
                       'macro_rules! m { ($c:expr,$method:ident) => { $c.$method(url) } }'):
            with self.subTest(source=source), self.assertRaises(ValueError):
                ban.lex.fingerprints(source)

    def test_pruning_is_allowed_but_growth_rehash_and_stale_allowances_fail(self):
        path = 'venues/example/src/lib.rs'
        a = dict(ban.lex.fingerprints('cx.connect(url);'))
        b = dict(ban.lex.fingerprints('other.connect(url);'))
        families = {'temporaryFeature': 'managed-sockets',
                    'sourceRoots': {'example': 'venues/example/src/'}}
        old = [("parent", {path: a})]
        self.assertFalse(ban.check({path: a}, {path: a}, families, old, [('parent', families)]))
        self.assertTrue(ban.check({path: b}, {path: b}, families, old))
        doubled = {key: count * 2 for key, count in a.items()}
        self.assertTrue(ban.check({path: doubled}, {path: doubled}, families, old))
        self.assertTrue(ban.check({}, {path: a}, families, old))
        empty = {'temporaryFeature': 'managed-sockets', 'sourceRoots': {}}
        self.assertFalse(ban.check({}, {}, empty, old, [('parent', families)]))

    def test_a_reviewed_argument_replacement_passes_once_and_nothing_else_does(self):
        path = 'venues/example/src/lib.rs'
        old = ban.lex.fingerprints('cx.connect(spec.feed(name, recover));')
        new = ban.lex.fingerprints('cx.connect(spec);')
        other = ban.lex.fingerprints('other.connect(url);')
        (old_digest,), (new_digest,) = old, new
        families = {'temporaryFeature': 'managed-sockets',
                    'sourceRoots': {'example': 'venues/example/src/'}}
        base = [('origin/base', {path: dict(old)})]
        entry = {path: [{'old': old_digest, 'new': new_digest}]}
        edited = {path: dict(new)}
        self.assertFalse(ban.check(edited, edited, families, base, replacements=entry))
        # Without the reviewed entry the edit is growth, exactly as today.
        self.assertTrue(ban.check(edited, edited, families, base))
        # An added site, or a count rise, still fails with the entry present.
        added = {path: {new_digest: 1, **dict(other)}}
        self.assertTrue(ban.check(added, added, families, base, replacements=entry))
        rise = {path: {new_digest: 2}}
        self.assertTrue(ban.check(rise, rise, families, base, replacements=entry))
        # The old digest must exist at the base; a replacement cannot mint a site.
        missing = [('origin/base', {path: dict(other)})]
        self.assertTrue(ban.check(edited, edited, families, missing, replacements=entry))
        # Single use: once the base carries the new digest, the entry is stale.
        landed = [('origin/base', {path: dict(new)})]
        self.assertTrue(ban.check(edited, edited, families, landed, replacements=entry))
        self.assertFalse(ban.check(edited, edited, families, landed))

    def test_replacement_entries_are_strictly_shaped(self):
        digest = 'a' * 64
        self.assertEqual(ban.replacement_manifest('{}'), {})
        for text in ('[]', '{"venues/x/src/lib.rs": []}',
                     '{"../x.rs": [{"old": "%s", "new": "%s"}]}' % (digest, 'b' * 64),
                     '{"venues/x/src/lib.rs": [{"old": "%s", "new": "%s"}]}' % (digest, digest),
                     '{"venues/x/src/lib.rs": [{"old": "%s"}]}' % digest,
                     '{"venues/x/src/lib.rs": [{"old": "x", "new": "%s"}]}' % digest):
            with self.subTest(text=text), self.assertRaises(ValueError):
                ban.replacement_manifest(text)

    def test_family_growth_root_changes_and_missing_families_fail(self):
        path = 'venues/example/src/lib.rs'
        current = {path: dict(ban.lex.fingerprints('cx.connect(url);'))}
        families = {'temporaryFeature': 'managed-sockets',
                    'sourceRoots': {'example': 'venues/example/src/'}}
        empty = {'temporaryFeature': 'managed-sockets', 'sourceRoots': {}}
        self.assertTrue(ban.check(current, current, families, old_families=[('parent', empty)]))
        self.assertTrue(ban.check(current, current, empty))
        changed = {'temporaryFeature': 'managed-sockets',
                   'sourceRoots': {'example': 'venues/example/src/account/'}}
        self.assertTrue(ban.check(current, current, changed, old_families=[('parent', families)]))

    def test_family_manifest_rejects_duplicate_keys_and_escaping_roots(self):
        for text in ('{"temporaryFeature":"managed-sockets","sourceRoots":{"x":"../"}}',
                     '{"temporaryFeature":"managed-sockets","sourceRoots":{},"sourceRoots":{}}',
                     '{"temporaryFeature":"other","sourceRoots":{}}'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                ban.family_manifest(text)

    def test_nested_family_cannot_hide_under_its_parent_root(self):
        parent = 'venues/example/src/lib.rs'
        nested = 'venues/example/src/spot/mod.rs'
        current = {path: dict(ban.lex.fingerprints('cx.connect(url);'))
                   for path in (parent, nested)}
        both = {'temporaryFeature': 'managed-sockets', 'sourceRoots': {
            'example': 'venues/example/src/', 'example-spot': 'venues/example/src/spot/'}}
        parent_only = {'temporaryFeature': 'managed-sockets', 'sourceRoots': {
            'example': 'venues/example/src/'}}
        self.assertTrue(ban.check(current, current, parent_only, old_families=[('parent', both)]))
        remaining = {parent: current[parent]}
        self.assertFalse(ban.check(remaining, remaining, parent_only,
                                   old_families=[('parent', both)]))

    def test_empty_inventory_requires_removing_even_quoted_feature_keys(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / 'scripts').mkdir()
            (root / 'engine').mkdir()
            (root / ban.SITES).write_text('{}')
            (root / ban.REPLACEMENTS).write_text('{}')
            (root / ban.FAMILIES).write_text(json.dumps({
                'temporaryFeature': 'managed-sockets', 'sourceRoots': {}}))
            cargo = root / 'engine/Cargo.toml'
            with mock.patch.object(ban, 'ROOT', root), \
                    mock.patch.object(ban.lex, 'sites', return_value={}), \
                    mock.patch.object(ban, 'previous', return_value=[]), \
                    mock.patch('builtins.print'):
                for key in ('managed-sockets', '"managed-sockets"', "'managed-sockets'",
                            '"managed\\u002dsockets"', '"managed\\U0000002dsockets"',
                            'features."managed-sockets"'):
                    cargo.write_text('[features]\n' + key + ' = []\n')
                    with self.subTest(key=key):
                        self.assertTrue(ban.main())
                cargo.write_text('features = { "managed-sockets" = [] }\n')
                self.assertTrue(ban.main())
                cargo.write_text('[features]\nother = []\n')
                self.assertFalse(ban.main())


if __name__ == '__main__':
    unittest.main()
