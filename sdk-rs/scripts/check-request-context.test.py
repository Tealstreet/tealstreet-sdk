"""Adversarial controls for the shrinking provenance inventory; no Rust build."""
import importlib.util
import json
import os
import pathlib
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location(
    'request_ban', pathlib.Path(__file__).with_name('check-request-context.py'))
ban = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ban)


class RequestContextBan(unittest.TestCase):
    def test_formatting_comments_and_literals(self):
        source = 'cx.request(Method::Get, "https://venue/read", pending);'
        wrapped = 'cx /* nested /* comment */ yes */ .\nrequest ( Method :: Get, "https://venue/read", pending );'
        self.assertEqual(ban.fingerprints(source), ban.fingerprints(wrapped))
        self.assertNotEqual(ban.fingerprints(source), ban.fingerprints(source.replace('cx.', 'other.')))
        self.assertNotEqual(ban.fingerprints(source), ban.fingerprints(source.replace('/read', '/write')))
        examples = r'''// cx.request(1)
            /* Cx::fail(1); /* nested */ cx.done(1); */
            let a = "escaped \" cx.request(1)";
            let b = r###"cx.request(1); \"###;
            let c = br#"Cx::fail(2)"#;
            let d = b'\''; let e = '\u{0027}';
            fn untouched<'a>(a: &'a str) {}
        '''
        self.assertFalse(ban.fingerprints(examples))

    def test_raw_qualified_generic_and_reference_forms_are_frozen(self):
        for source in [
            'cx.r#request(1);', 'Cx::r#fail(1);', 'Cx::request(cx, 1);',
            '<Cx as Trait>::request(cx, 1);', 'Alias::done(cx, 1);',
            'cx.request::<Vec<Vec<u32>>>(1);', 'let send = (Cx::request);',
            '(cx).request(1);', 'let send = Alias::request::<Pending>;',
        ]:
            with self.subTest(source=source):
                found = ban.fingerprints(source)
                self.assertEqual(sum(found.values()), 1)
                self.assertEqual(sum(ban.fingerprints(source + source).values()), 2)

    def test_macro_indirection_and_include_cannot_bypass_inventory(self):
        for source in [
            'call!(cx, request);', 'call! { cx, r#fail }',
        ]:
            with self.subTest(source=source):
                self.assertTrue(ban.fingerprints(source))
        self.assertFalse(ban.fingerprints('example!("cx.request(1)");'))
        # Moving one frozen call into a repeatedly invoked macro cannot keep the
        # same fingerprint/count and multiply its physical opaque callers.
        before = 'cx.done(command, "null".into());'
        self.assertTrue(ban.fingerprints(before))
        for source in [
            'macro_rules! repeat { ($cx:ident) => { $cx.done(command, "null".into()); } } repeat!(cx); repeat!(cx);',
            'macro_rules! send { ($cx:expr,$method:ident) => { $cx.$method(1) } }',
            'include!(concat!(env!("OUT_DIR"), "/generated.rs"));',
        ]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                ban.fingerprints(source)

    def test_malformed_lexical_input_fails_closed(self):
        for source in ['/* missing', 'r##"missing"#', '"missing', 'cx.request(1',
                       'cx.request::<Vec<u8>(1);', '(cx].request(1);']:
            with self.subTest(source=source), self.assertRaises(ValueError):
                ban.fingerprints(source)

    def test_exact_inventory_and_independent_shrink_ratchet(self):
        path = 'venues/reference/src/lib.rs'
        one = {path: dict(ban.fingerprints('cx.request(1);'))}
        two = {path: dict(ban.fingerprints('cx.request(1); cx.request(2);'))}
        self.assertFalse(ban.check(one, one, [('parent', two)]))
        self.assertTrue(ban.check(two, two, [('parent', one)]))
        self.assertTrue(ban.check(one, two, [('parent', two)]))
        self.assertTrue(ban.check(two, one, [('parent', two)]))
        self.assertFalse(ban.check({}, {}, [('base', one)]))
        # Bootstrap only establishes the reviewed initial inventory.
        self.assertFalse(ban.check(one, one, []))

    def test_strict_manifest_schema_and_duplicates(self):
        digest = 'a' * 64
        good = {'venues/reference/src/lib.rs': {digest: 1}}
        self.assertEqual(ban.manifest(json.dumps(good)), good)
        for value in [[], {'x.rs': {digest: 1}}, {'venues/a/src/../b.rs': {digest: 1}},
                      {'venues/a/src/b.rs': {}}, {'venues/a/src/b.rs': {digest: True}},
                      {'venues/a/src/b.rs': {digest: 0}}, {'venues/a/src/b.rs': {'bad': 1}}]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                ban.manifest(json.dumps(value))
        for text in ['{"x":{},"x":{}}', '{"venues/a/src/b.rs":{"' + digest + '":1,"' + digest + '":2}}']:
            with self.assertRaises(ValueError):
                ban.manifest(text)

    def test_base_ref_and_exact_event_commit_are_required(self):
        sha = '1' * 40
        calls = []

        def fake_git(*args):
            calls.append(args)
            if args[0] == 'rev-parse' and args[1] == '--show-prefix':
                return 'sdk-rs/\n'
            if args[0] == 'rev-list':
                return ' '.join(letter * 40 for letter in ['a', 'b', 'c'])
            return ''

        with tempfile.TemporaryDirectory() as directory:
            event = pathlib.Path(directory) / 'event.json'
            event.write_text(json.dumps({'pull_request': {'base': {'sha': sha}}}))
            with mock.patch.dict(os.environ, {'GITHUB_EVENT_PATH': str(event), 'GITHUB_BASE_REF': 'landing'}):
                with mock.patch.object(ban, 'git', side_effect=fake_git):
                    self.assertEqual(ban.previous_manifests(), [])
                self.assertIn(('rev-parse', '--verify', sha + '^{commit}'), calls)
                self.assertIn(('rev-parse', '--verify', 'refs/remotes/origin/landing^{commit}'), calls)
                for parent in ['b' * 40, 'c' * 40]:
                    self.assertIn(('rev-parse', '--verify', parent + '^{commit}'), calls)
                with mock.patch.object(ban, 'git', side_effect=subprocess.CalledProcessError(1, 'git')):
                    with self.assertRaises(subprocess.CalledProcessError):
                        ban.previous_manifests()

    def test_real_git_baselines_resolve_from_sdk_subdirectory_and_block_committed_growth(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = pathlib.Path(directory)
            root = repo / 'sdk-rs'
            source = root / 'venues/reference/src/lib.rs'
            source.parent.mkdir(parents=True)
            inventory = root / ban.MANIFEST
            inventory.parent.mkdir(parents=True)

            def command(*args):
                return subprocess.run(['git', '-C', str(repo), *args], check=True,
                                      capture_output=True, text=True).stdout.strip()

            def commit():
                command('add', 'sdk-rs')
                command('-c', 'user.name=Ratchet', '-c', 'user.email=ratchet@example.invalid',
                        'commit', '--allow-empty', '-m', 'fixture')

            command('init')
            with mock.patch.object(ban, 'ROOT', root), mock.patch.dict(os.environ, {}, clear=True):
                source.write_text('cx.request(1);')
                first = ban.sites()
                inventory.write_text(json.dumps(first))
                commit()
                command('update-ref', 'refs/remotes/origin/feat/rust-rewrite', 'HEAD')
                commit()
                previous = ban.previous_manifests()
                self.assertEqual(len(previous), 3)
                self.assertTrue(all(value == first for _, value in previous))
                self.assertFalse(ban.check(ban.sites(), first, previous))
                source.write_text('cx.request(1); cx.request(2);')
                expanded = ban.sites()
                inventory.write_text(json.dumps(expanded))
                self.assertTrue(ban.check(expanded, expanded, ban.previous_manifests()))
                commit()
                commit()
                # Even after two commits make HEAD and parent agree, landing
                # remains an independent trusted shrinking baseline.
                self.assertTrue(ban.check(expanded, expanded, ban.previous_manifests()))


if __name__ == '__main__':
    unittest.main()
