"""Adversarial controls for the shrinking provenance inventory; no Rust build."""
import importlib.util
import hashlib
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

    def test_snapshot_pending_rebind_is_exact_and_cannot_allow_regrowth(self):
        path = 'venues/paradex/src/account.rs'
        before, after = ban.HISTORY_BOOTSTRAPS[path]
        old, current = {path: dict(before)}, {path: dict(after)}
        self.assertEqual(sum(before.values()), 10)
        self.assertEqual(sum(after.values()), 10)
        self.assertEqual(hashlib.sha256(json.dumps(before, sort_keys=True).encode()).hexdigest(),
                         '2533bbc9d874e500f4a9a6c47b47f0c36770957f564cc9d8523629f0e4b1af2c')
        self.assertEqual(hashlib.sha256(json.dumps(after, sort_keys=True).encode()).hexdigest(),
                         '6318d427aca63f36566df341d8ed3facc9b9f49b68a0d08f4d774567b83df9a9')
        original = ban.git('show', '848924471635eee7a0c61c64407a861c3aea37f3:sdk-rs/' + path)
        current_source = (ban.ROOT / path).read_text()
        self.assertEqual(dict(ban.fingerprints(original)), before)
        self.assertEqual(dict(ban.fingerprints(current_source)), after)
        self.assertEqual(set(before) - set(after), {
            '5c28b0ebae1f2a54f344d52c2c422801c8e82381c7fd8ad95d13cd549e88ddec'})
        self.assertEqual(set(after) - set(before), {
            '29a6633fa3f6e05f04f534c0e675690014adde1e8a26e93c3769e8618602c13c'})
        self.assertFalse(ban.check(current, current, [('old', old)]))
        self.assertFalse(ban.check(current, current, [('landed', current)]))
        extra = {path: {**after, 'f' * 64: 1}}
        self.assertTrue(ban.check(extra, extra, [('old', old)]))
        increased = {path: {**after}}
        increased[path][next(iter(after))] += 1
        self.assertTrue(ban.check(increased, increased, [('old', old)]))
        pruned = {path: {**after}}
        del pruned[path][next(iter(after))]
        self.assertTrue(ban.check(current, current, [('old', old), ('parent', pruned)]))
        altered_old = {path: {**before}}
        del altered_old[path][next(iter(before))]
        self.assertTrue(ban.check(current, current, [('altered', altered_old)]))
        unrelated = {**current, 'venues/other/src/account.rs': dict(after)}
        self.assertTrue(ban.check(unrelated, unrelated, [('old', old)]))
        mismatch = {path: {**after}}
        del mismatch[path][next(iter(after))]
        self.assertTrue(ban.check(mismatch, current, [('old', old)]))

    def test_exact_landed_bootstrap_transition_never_allows_regrowth(self):
        path = ban.BOOTSTRAP_PATH
        old = {path: dict(ban.BOOTSTRAP_OLD)}
        landed = {path: dict(ban.BOOTSTRAP_LANDED)}
        self.assertEqual(sum(old[path].values()), 19)
        self.assertEqual(sum(landed[path].values()), 14)
        self.assertFalse(ban.check(landed, landed, [('original', old)]))
        pruned = {path: dict(landed[path])}
        digest = next(iter(pruned[path]))
        del pruned[path][digest]
        self.assertFalse(ban.check(pruned, pruned, [('original', old), ('landed', landed)]))
        # An older original map must not override a newer, independently pruned
        # parent/base. It remains a permanent independent shrinking constraint.
        self.assertTrue(ban.check(landed, landed, [('original', old), ('parent', pruned)]))
        altered = {path: dict(old[path])}
        altered[path][next(iter(altered[path]))] += 1
        self.assertTrue(ban.check(landed, landed, [('altered', altered)]))
        extra = {path: dict(landed[path])}
        extra[path]['f' * 64] = 1
        self.assertTrue(ban.check(extra, extra, [('original', old)]))
        increased = {path: dict(landed[path])}
        increased[path][next(iter(increased[path]))] += 1
        self.assertTrue(ban.check(increased, increased, [('original', old)]))
        other = dict(landed)
        other['venues/other/src/history.rs'] = dict(landed[path])
        self.assertTrue(ban.check(other, other, [('original', old)]))
        # Source changes still have to exactly match the frozen manifest.
        self.assertTrue(ban.check(extra, landed, [('original', old)]))

    def test_exact_signer_factory_rehash_preserves_count_and_cannot_regrow(self):
        approved = {'venues/binance/src/history.rs': {'after': {'153ef00b195a92e18337317fde397c0707f7778eadc4bfc02b2c0d2ffbf493c5': 1,
                                                     '2263b7c3975168a800233a5720729a5293adc8d79ddc822742da439875b1c927': 1,
                                                     '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 3,
                                                     '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 1,
                                                     '6a3d37fddbb5fff8f3cd2c8d2ab4958d84b5c1efd268fd69c1464b41183a0c6a': 1,
                                                     '896c2dc203c518dc9ae1c6b5eb51b5591da28af19f9ff2461c4c629a490fb1a5': 1,
                                                     '931fea11387a3eb1ccf2c909b869b3ee559925055e436a3987df14969b35fc81': 1,
                                                     'a2e0593410ad976880489badf3fbdf89ad9f79859b04b9860d23044f327967d3': 1,
                                                     'abe91b4dc9b434e9eb3b21058d48e235a63990574ed4431b56d81aa1bb372f49': 1,
                                                     'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 2,
                                                     'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
                                                     'f859db20866f37469af70c624ec5ad2f31c4f7bd10c1bd1b0da41299b91f7eb7': 1},
                                           'before': {'153ef00b195a92e18337317fde397c0707f7778eadc4bfc02b2c0d2ffbf493c5': 1,
                                                      '2263b7c3975168a800233a5720729a5293adc8d79ddc822742da439875b1c927': 1,
                                                      '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 3,
                                                      '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 1,
                                                      '6a3d37fddbb5fff8f3cd2c8d2ab4958d84b5c1efd268fd69c1464b41183a0c6a': 1,
                                                      '7b98ff7fe9c2b0fba3fa2034b1d3cddb9008a47f9c7731d28364b7097fb1b66d': 1,
                                                      '896c2dc203c518dc9ae1c6b5eb51b5591da28af19f9ff2461c4c629a490fb1a5': 1,
                                                      'a2e0593410ad976880489badf3fbdf89ad9f79859b04b9860d23044f327967d3': 1,
                                                      'abe91b4dc9b434e9eb3b21058d48e235a63990574ed4431b56d81aa1bb372f49': 1,
                                                      'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 2,
                                                      'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
                                                      'f859db20866f37469af70c624ec5ad2f31c4f7bd10c1bd1b0da41299b91f7eb7': 1}},
         'venues/binance/src/lib.rs': {'after': {'088924f3ac9b1699da1a2b0c712052ec5d14e133e51b4a93094827c12109ef7e': 1,
                                                 '29866cd39178c8da46ea6e7de031a9cd684e76f2734dbc6f60a081be3980972b': 1,
                                                 '5ed40af7dbe1e5b76ca8be90e29eabcbce35d0d0c768e765826c2b6525a06ada': 1,
                                                 '931fea11387a3eb1ccf2c909b869b3ee559925055e436a3987df14969b35fc81': 1,
                                                 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1},
                                       'before': {'088924f3ac9b1699da1a2b0c712052ec5d14e133e51b4a93094827c12109ef7e': 1,
                                                  '29866cd39178c8da46ea6e7de031a9cd684e76f2734dbc6f60a081be3980972b': 1,
                                                  '5ed40af7dbe1e5b76ca8be90e29eabcbce35d0d0c768e765826c2b6525a06ada': 1,
                                                  '660826b694da348256b3fbbb3d2e4429c0be08068f931faa9b0cf92bae92004d': 1,
                                                  'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1}}}
        original = {path: dict(pair['before']) for path, pair in approved.items()}
        changed = {path: dict(pair['after']) for path, pair in approved.items()}
        self.assertEqual(sum(map(sum, (c.values() for c in original.values()))), 21)
        self.assertEqual(sum(map(sum, (c.values() for c in changed.values()))), 21)
        self.assertFalse(ban.check(changed, changed, [('approved-original', original)]))
        for path in changed:
            pruned = {p: dict(c) for p, c in changed.items()}
            del pruned[path][next(iter(pruned[path]))]
            self.assertTrue(ban.check(changed, changed, [('original', original), ('newer-pruned', pruned)]))
            self.assertFalse(ban.check(pruned, pruned, [('original', original), ('newer', changed)]))
            forged = {p: dict(c) for p, c in changed.items()}
            forged[path]['f' * 64] = 1
            self.assertTrue(ban.check(forged, forged, [('original', original)]))
            grown = {p: dict(c) for p, c in changed.items()}
            grown[path][next(iter(grown[path]))] += 1
            self.assertTrue(ban.check(grown, grown, [('original', original)]))
            altered_original = {p: dict(c) for p, c in original.items()}
            altered_original[path][next(iter(altered_original[path]))] += 1
            self.assertTrue(ban.check(changed, changed, [('altered-original', altered_original)]))
            moved = {p: dict(c) for p, c in changed.items()}
            moved['venues/unapproved/src/lib.rs'] = moved.pop(path)
            self.assertTrue(ban.check(moved, moved, [('original', original)]))
        self.assertTrue(ban.check(original, original, [('already-rehashed', changed)]))
        self.assertTrue(ban.check(changed, original, [('original', original)]))

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

    def test_pinned_history_bootstraps_are_net_shrinking_and_exact_landing_maps(self):
        expected = {
            'venues/coinbase/src/history.rs': (7, 3, '7f923fa1fa91b9f81e47a240cadedad920dcad577279ab675d23b2f4a84ffb54', 'd6fe784f25ab245b2860a9f6d46c3617cbc1c4545197e29a140dadeca7e91372'),
            'venues/weex/src/history.rs': (17, 10, '28fb5096b49ad5e340d77654b9b7b0bc4b5922b4dc58af5cff5fcfdff2161327', '477c91d6c2d026dea59a81d4dc539b56985dadf3606bdb7eae5b4ed505936163'),
            'venues/cryptocom/src/history.rs': (15, 10, '8c08399ee9544d819511127584929ff3a4d532831d64ffc1886424efb4842b0c', '0716d0d13befbecad3402c21949f8d88fa30f02b9a761b1d3dbeae65570526f6'),
            'venues/grvt/src/history.rs': (13, 11, 'f97e2d042aad67c83cb7b67fd2e18fe2ea1911d39bee4928b4bfcd29a53a9173', '10b79560bfa80b935be2f78f7f6a3fb2e7b44436372c89a588a15d558c07fa3e'),
            'venues/kucoin/src/spot/mod.rs': (22, 20, 'cff9fea5f76ae51654a83c7953df010ef41e3941614a6fdd67b9f1001bd212ff', 'fca911a5d114175c7374dfef35111bf3498f871eead93ef398d926ccf6929da8'),
        }
        self.assertEqual(set(expected) | {'venues/hyperliquid/src/history.rs',
                                          'venues/paradex/src/account.rs'}, set(ban.HISTORY_BOOTSTRAPS))
        for path, (before, after, old_hash, landed_hash) in expected.items():
            with self.subTest(path=path):
                old, landed = ban.HISTORY_BOOTSTRAPS[path]
                self.assertEqual((sum(old.values()), sum(landed.values())), (before, after))
                self.assertLess(after, before)
                for value, digest in [(old, old_hash), (landed, landed_hash)]:
                    self.assertEqual(hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest(), digest)
                ref = ('fe44969d033529c244fc4f33fce5395c0273df04'
                       if path in {'venues/cryptocom/src/history.rs', 'venues/grvt/src/history.rs'}
                       else '5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6')
                if path == 'venues/kucoin/src/spot/mod.rs':
                    ref = '4a185e6459856ed27f7753b95a74b59d36dc3324'
                source = ban.git('show', ref + ':sdk-rs/' + path)
                self.assertEqual(dict(ban.fingerprints(source)), landed)
                if ref == '4a185e6459856ed27f7753b95a74b59d36dc3324':
                    original = ban.git('show', 'b2fe97324763e1e2702c69ac5b839da37102928b:sdk-rs/' + path)
                    self.assertEqual(dict(ban.fingerprints(original)), old)
                if ref == 'fe44969d033529c244fc4f33fce5395c0273df04':
                    original = ban.git('show', 'a0c528f910efed5993b63abe8ded522b2f7c26ee:sdk-rs/' + path)
                    self.assertEqual(dict(ban.fingerprints(original)), old)
                    def kinds(text):
                        stream = ban.tokens(text)
                        return {token.removeprefix('r#') for index, token in enumerate(stream)
                                if index and stream[index - 1] in {'.', '::'}
                                and token.removeprefix('r#') in ban.METHODS}
                    self.assertLessEqual(kinds(source), kinds(original))

    def test_staging_bootstraps_add_only_the_approved_sites(self):
        # Owner approvals: exact added fingerprints and exact entry sizes.
        sign_http = '6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219'
        approved = {
            'venues/bingx/src/lib.rs': ({sign_http}, 10, 9),
            'venues/weex/src/lib.rs': ({sign_http}, 19, 17),
            'venues/okx/src/lib.rs': ({
                '2e9218906fd923fa6c83c648873ea2527c33dd1068d609c8c1ec343630b36911',
                'b1761cdefa174ea70037cfceab33b91ad7279e59d22ccecb872e996dd5b0c32c',
                'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa'}, 8, 11),
            'venues/bitget/src/lib.rs': ({
                '5b44116449d06833b5f091cb3866ced7f16f181fd1226b31ad977eff6bf26ac9',
                '6c77d608b079104cc5338cbabb01a7c4a0e24f112ef91ea5483c423808d92bad'}, 4, 6),
        }
        self.assertEqual(set(ban.STAGING_BOOTSTRAPS), set(approved))
        for path, (old_map, new_map) in ban.STAGING_BOOTSTRAPS.items():
            with self.subTest(path=path):
                added = {d for d, n in new_map.items() if n > old_map.get(d, 0)}
                approved_added, old_size, new_size = approved[path]
                self.assertEqual(added, approved_added)
                self.assertEqual((sum(old_map.values()), sum(new_map.values())), (old_size, new_size))
                old, new = {path: old_map}, {path: new_map}
                self.assertFalse(ban.check(new, new, [('landed', old)]))
                grown = [dict(new_map, **{'f' * 64: 1})]
                grown += [{**new_map, d: new_map[d] + 1} for d in sorted(added)]
                for map_ in grown:
                    self.assertTrue(ban.check({path: map_}, {path: map_}, [('landed', old)]))
                for digest in sorted(added):
                    pruned_map = dict(new_map)
                    del pruned_map[digest]
                    pruned = {path: pruned_map}
                    # Once a staging site is pruned, nothing can restore it.
                    self.assertTrue(ban.check(new, new, [('landed', old), ('pruned', pruned)]))

    def test_history_bootstraps_cannot_expand_or_override_any_newer_pruned_constraint(self):
        for path, (old_map, new_map) in ban.HISTORY_BOOTSTRAPS.items():
            with self.subTest(path=path):
                old, landed = {path: old_map}, {path: new_map}
                self.assertFalse(ban.check(landed, landed, [('original', old)]))
                self.assertTrue(ban.check(old, old, [('already-landed', landed)]))
                digest = next(iter(new_map))
                pruned_map = dict(new_map)
                del pruned_map[digest]
                pruned = {path: pruned_map}
                self.assertFalse(ban.check(pruned, pruned, [('original', old), ('landed', landed)]))
                self.assertTrue(ban.check(landed, landed, [('original', old), ('pruned', pruned)]))
                for altered in [dict(new_map, **{'f' * 64: 1}), {**new_map, digest: new_map[digest] + 1}]:
                    extra = {path: altered}
                    self.assertTrue(ban.check(extra, extra, [('original', old)]))
                altered_old = {**old_map, 'e' * 64: 1}
                self.assertTrue(ban.check(landed, landed, [('altered baseline', {path: altered_old})]))
                wrong_path = {'venues/other/src/history.rs': new_map}
                self.assertTrue(ban.check(wrong_path, wrong_path, [('original', old)]))
                self.assertTrue(ban.check(wrong_path, landed, [('original', old)]))

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


    def test_exact_hyperliquid_history_maps_and_call_kinds(self):
        path = 'venues/hyperliquid/src/history.rs'
        old, new = ban.HISTORY_BOOTSTRAPS[path]
        self.assertEqual((sum(old.values()), sum(new.values())), (21, 15))
        self.assertEqual(hashlib.sha256(json.dumps(old, sort_keys=True).encode()).hexdigest(), '18415637b13f7300f84e1a1e4164527e006f627946bf5f22ec18089f41351044')
        self.assertEqual(hashlib.sha256(json.dumps(new, sort_keys=True).encode()).hexdigest(), '750e200bc590f090b632d978cdfd62a47711e028be67618c956c04f08280c8a2')
        original = ban.git('show', 'b2fe97324763e1e2702c69ac5b839da37102928b' + ':sdk-rs/' + path)
        reviewed = ban.git('show', '400e12b5e1258c53025990e7826f46cb3e3efdfb' + ':sdk-rs/' + path)
        self.assertEqual(dict(ban.fingerprints(original)), old)
        self.assertEqual(dict(ban.fingerprints(reviewed)), new)

if __name__ == '__main__':
    unittest.main()
