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




# Venue helpers whose matching parameter is unambiguously Cx-typed.
HELPERS = ("fn request(&mut self, cx: &mut Cx<'_, P>, command: u64) {} "
           "fn fail(&mut self, cx: &mut Cx<'_, P>, command: u64, error: E) {} "
           "fn helper(cx: &mut Cx<'_, P>, n: u32) {} fn other(cx: &mut Cx<'_, P>) {} "
           "fn bump(&mut self, cx: &mut Cx<'_, P>) {} ")


def in_cx_fn(body, name='cx'):
    return ('fn handle(&mut self, account: &mut Account, ' + name + ": &mut Cx<'_, Pending>) { "
            + body + ' }')


class PrecisionRed(unittest.TestCase):
    """RUST_REQUEST_CONTEXT_PRECISION.md: only resolved Cx calls are legacy."""

    def prod(self, source):
        prod, test = ban.context_fingerprints(source)
        self.assertFalse(test)
        return prod

    def test_conservative_matches_record_nothing(self):
        for body in [
            'if matches!(request, Request::Get) { return; }',
            'let label = format!("{} {}", request.path, done);',
            'self.request(cx, command);', 'self.fail(cx, command, error);',
            'pending.request.retry();', 'h.done(row);', 'list.done();',
            'sign::request(&secret, payload);', 'assert!(row.done());',
        ]:
            with self.subTest(body=body):
                self.assertTrue(ban.fingerprints(in_cx_fn(body)))
                self.assertFalse(self.prod(HELPERS + in_cx_fn(body)))

    def test_receiver_is_resolved_by_type_not_name(self):
        call = 'c.done(command, "null".into());'
        lexical = ban.fingerprints('c.done(command, "null".into());')
        self.assertEqual(self.prod(in_cx_fn(call, 'c')), lexical)
        self.assertEqual(self.prod("fn f(c: &mut safe_cex_engine::Cx<'_, P>) { " + call + ' }'), lexical)
        self.assertEqual(self.prod('fn f() { each(|c, row| { ' + call + ' }); }'), lexical)
        self.assertFalse(self.prod('fn f(c: &mut Other) { ' + call + ' }'))
        # The innermost binding wins: a typed non-Cx closure parameter shadows.
        self.assertFalse(self.prod(in_cx_fn('each(|c: &mut Row| { ' + call + ' });', 'c')))
        # An inferred closure type is untyped, so it is counted.
        for kind in ['_', '&mut _', "&'a mut _", 'Option<_>', '&mut Vec<_>', '&mut S<_>']:
            self.assertEqual(self.prod('fn f() { each(|c: ' + kind + '| { ' + call + ' }); }'), lexical)
        # One predicate allows the type and resolves the receiver, lifetimes included.
        for kind in ["&'a Cx<'_, P>", "&'a mut Cx<'_, P>", "&Cx<'_, P>"]:
            self.assertEqual(self.prod('fn f<\'a>(c: ' + kind + ') { ' + call + ' }'), lexical)
        # Real sites keep their exact fingerprint, so the rebaseline is a subset.
        real = 'cx.request(Method::Get, "https://venue/read", pending);'
        self.assertEqual(self.prod(in_cx_fn(real)), ban.fingerprints(real))

    def test_qualified_cx_forms_stay_counted(self):
        for body in ['Cx::request(cx, 1);', '<Cx as Trait>::fail(cx, 1);',
                     'Alias::done(cx, 1);', 'let send = Cx::fail;',
                     'safe_cex_engine::Cx::request(cx, 1);']:
            with self.subTest(body=body):
                self.assertEqual(sum(self.prod(in_cx_fn(body)).values()), 1)
        self.assertFalse(self.prod(in_cx_fn('sign::request(secret, 1);')))

    def test_cx_type_names_outside_parameters_fail_closed(self):
        for source in [
            "use safe_cex_engine::Cx as Ctx;",
            "type Alias<'a> = Cx<'a, Pending>;",
            "struct Holder<'a> { cx: &'a mut Cx<'a, Pending> }",
            "fn f(cx: Option<&mut Cx<'_, Pending>>) {}",
            "fn f<'a>(cx: &'a mut Cx<'_, Pending>) -> &'a mut Cx<'_, Pending> { cx }",
            "impl Legacy for Cx<'_, Pending> {}",
        ]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                ban.context_fingerprints(source)
        ban.context_fingerprints("fn f(apply: impl FnMut(&mut Cx<'_, Pending>, Row)) {}")

    def test_an_escaping_cx_counts_every_reference_in_its_function(self):
        # ESCAPE (claude-ddi2il): any use but a member or an analysed call argument
        # taints the function; every legacy reference in it counts as before.
        unrelated = 'h.done(1); pending.request.retry();'
        for escape in [
            'let c = cx;', 'let c = &mut *cx;', '(cx).now;', 'let s = S { cx };', 'let s = S { inner: cx };',
            'let pair = (cx, 1);', 'let all = [cx];', 'match cx { _ => {} }', 'if let c = cx { }',
            'm!(cx);', 'm![cx, 1];', 'm! { cx }', 'store(Some(cx));', 'let w = W(cx);',
            'let c = pass(cx);', 'x = pass(cx);', 'pass(cx).now;', 'v.push(cx);', 'h.set(cx);',
            'v.push(W(cx));', 'v.extend(Some(cx));', 'h.x = Some(cx);', 'v[0] = Some(cx);', '*s = Some(cx);',
            'std::mem::replace(&mut slot, cx);', 'Vec::push(&mut v, cx);', 'keep(move || cx.now);',
        ]:
            with self.subTest(escape=escape):
                self.assertEqual(sum(self.prod(in_cx_fn(escape + ' ' + unrelated)).values()), 2)
        # Without an escape the same unrelated references record nothing.
        for analysed in ['helper(cx, 1);', 'self.bump(cx);', 'account.bump(cx);',
                         'other(&mut *cx);', 'return other(cx);', 'let t = cx.now;']:
            with self.subTest(analysed=analysed):
                self.assertFalse(self.prod(HELPERS + in_cx_fn(analysed + ' ' + unrelated)))

    def test_a_cx_beside_another_non_literal_argument_needs_an_unambiguous_cx_parameter(self):
        # ESCAPE clause (1): sole non-literal argument, or every same-named fn takes a Cx there.
        generic = 'fn stash<T>(v: &mut Vec<T>, t: T) {} '
        unrelated = ' h.done(1);'
        for body in ['let vr = &mut v; stash(vr, cx); v[0].request(1);',
                     'stash(&mut *vr, cx); v[0].request(1);', 'stash(v.as_mut(), cx); v[0].request(1);']:
            with self.subTest(body=body):
                self.assertEqual(sum(self.prod(generic + in_cx_fn(body + unrelated)).values()), 2)
        # A generic stash shadowed by a Cx-typed stash elsewhere is still ambiguous.
        shadowed = generic + "fn stash(v: &mut Vec<u8>, cx: &mut Cx<'_, P>) {} "
        self.assertEqual(sum(self.prod(shadowed + in_cx_fn('stash(vr, cx);' + unrelated)).values()), 1)
        # Unresolved names escape, even as the sole argument (allowlist); an
        # unambiguous Cx parameter is analysed.
        for body in ['unknown(vr, cx);', 'unknown(cx, 1, "x");', 'unknown(cx);']:
            with self.subTest(body=body):
                self.assertEqual(sum(self.prod(in_cx_fn(body + unrelated)).values()), 1)
        typed = "fn record(v: &mut Vec<u8>, cx: &mut Cx<'_, P>) {} "
        self.assertFalse(self.prod(typed + in_cx_fn('record(vr, cx);' + unrelated)))

    def test_only_an_allowlisted_resolved_callee_is_analysed(self):
        unrelated = ' h.done(1);'
        sink = 'fn mk<\'a, T>(v: &\'a mut Vec<T>) -> impl FnMut(T) + \'a { move |t| v.push(t) } '
        relay = "fn apply<F: FnMut(&mut Cx<'_, P>)>(cx: &mut Cx<'_, P>, mut g: F) { g(cx); } "
        replace = "fn replace(&mut self, cx: &mut Cx<'_, P>) {} "
        for source in [
            # Closures however bound (4yjlw0 P1).
            sink + in_cx_fn('if let Some(mut g) = Some(mk(&mut v)) { g(cx); }' + unrelated),
            sink + in_cx_fn('let (mut g,) = (mk(&mut v),); g(cx);' + unrelated),
            sink + in_cx_fn('for mut g in [mk(&mut v)] { g(cx); }' + unrelated),
            # A resolved callee whose own Cx escapes is no analysed callee.
            sink + relay + in_cx_fn('apply(cx, mk(&mut v));' + unrelated),
            # Paths and renames never resolve (P2), even beside a namesake method.
            replace + in_cx_fn('std::mem::replace(&mut slot, cx);' + unrelated),
            replace + 'use std::mem::replace as other; ' + HELPERS + in_cx_fn('other(cx);' + unrelated),
            # A name bound locally shadows the fn item (addendum a).
            HELPERS + in_cx_fn('let other = |c| v.push(c); other(cx);' + unrelated),
            HELPERS + "fn f(other: impl FnMut(&mut Cx<'_, P>), cx: &mut Cx<'_, P>) { other(cx);" + unrelated + ' }',
            # Any import of the name, or any glob, may name something else (addendum b).
            'use crate::sink::other; ' + HELPERS + in_cx_fn('other(cx);' + unrelated),
            'use super::*; ' + HELPERS + in_cx_fn('other(cx);' + unrelated),
        ]:
            with self.subTest(source=source):
                self.assertGreaterEqual(sum(self.prod(source).values()), 1)
                self.assertIn(ban.fingerprints('h.done(1);').popitem()[0], self.prod(source))

    def test_a_macro_generated_fn_fails_closed(self):
        source = ('macro_rules! gen { ($n:ident) => { fn $n<T>(t: T, v: &mut Vec<T>) { v.push(t) } } } gen!(keep); '
                  "fn keep(cx: &mut Cx<'_, P>, n: u8) {} " + in_cx_fn('keep(cx, &mut v); v[0].request(1);'))
        with self.assertRaises(ValueError):
            ban.context_fingerprints(source)

    def test_closure_escapes_taint_the_enclosing_function(self):
        unrelated = ' h.done(1);'
        for body in ['let mut g = |c| v.push(c); g(cx); v[0].request(1);',
                     'run(cx, |c| v.push(c)); v[0].request(1);']:
            with self.subTest(body=body):
                self.assertEqual(sum(self.prod(in_cx_fn(body + unrelated)).values()), 2)

    def test_a_generic_escape_counts(self):
        # A venue generic identity returns the Cx under another name.
        source = 'fn id<T>(t: T) -> T { t } ' + in_cx_fn('let c = id(cx); c.request(1);')
        self.assertEqual(sum(self.prod(source).values()), 1)

    def test_cx_flowing_through_expressions_and_bindings_is_counted(self):
        for body in [
            'pass(cx).request(1);', 'Wrap(cx).0.done(1, row);', 'cells[index(cx)].fail(1, e);',
            'let c = Some(cx).unwrap(); c.request(1);', 'let c: _ = pass(cx); c.done(1, row);',
            'for c in [pass(cx)] { c.fail(1, e); }', 'if let Some(c) = wrap(cx) { c.done(1, row); }',
            'match wrap(cx) { Some(c) => c.done(1, row), None => {} }',
            'each(|c, row| { let d = c; d.done(1, row); });',
            # Chains rooted at a derived binding, and plain reassignment.
            'let w = W(cx); w.0.request(1);', 'let h = H::new(cx); h.cx.done(1, r);',
            'let w = pass(cx); w.inner().request(1);', 'let (w,) = (W(cx),); w.0.request(1);',
            'let mut c = other(); c = pass(cx); c.request(1);',
            # A local that a method call may store the Cx into.
            'let mut v = Vec::new(); v.push(cx); v[0].request(1);',
            'let mut h = H::default(); h.set(cx); h.cx.request(1);',
        ]:
            with self.subTest(body=body):
                self.assertEqual(sum(self.prod(in_cx_fn(body)).values()), 1)
        # Members of a Cx field are not Cx calls; field reads are not calls at all.
        # A concrete non-Cx typed binding cannot store a Cx without naming its type.
        for body in ['cx.store.done(1);', 'let s = cx.now; s.done(1);', 'if read.request.done { }',
                     'account.bump(cx); account.request(cx, 1);']:
            with self.subTest(body=body):
                self.assertFalse(self.prod(HELPERS + in_cx_fn(body)))

    def test_a_macro_carrying_a_cx_value_taints_its_function(self):
        source = 'macro_rules! id { ($e:expr) => { $e } } ' + in_cx_fn('id!(cx).fail(1, e);')
        self.assertEqual(sum(self.prod(source).values()), 1)
        # Calls and members inside a macro are ordinary uses.
        # A member inside a macro is an ordinary use; a nested call's result is consumed.
        self.assertFalse(self.prod(in_cx_fn('log!(cx.now); h.done(1);')))
        self.assertTrue(self.prod(in_cx_fn('let s = format!("{}", label(cx, 1)); h.done(1);')))

    def test_test_regions_are_listed_separately(self):
        call = 'cx.done(command, "null".into());'
        expected = ban.fingerprints(call)
        for source in [
            '#[cfg(test)] mod tests { use super::*; ' + in_cx_fn(call) + ' }',
            '#[cfg(test)] ' + in_cx_fn(call),
            '#[cfg(test)] #[allow(dead_code)] ' + in_cx_fn(call),
        ]:
            with self.subTest(source=source):
                self.assertEqual(ban.context_fingerprints(source), ({}, expected))
        for attribute in ['#[cfg(feature = "x")]', '#[cfg(any(test, feature = "x"))]',
                          '#[cfg(not(test))]', '#[cfg_attr(test, allow(dead_code))]']:
            with self.subTest(attribute=attribute):
                self.assertEqual(ban.context_fingerprints(attribute + ' ' + in_cx_fn(call)), (expected, {}))
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            src = root / 'venues/reference/src'
            (src / 'history').mkdir(parents=True)
            (src / 'lib.rs').write_text('mod history; #[cfg(test)] mod secondary_tests; ' + in_cx_fn(call))
            (src / 'history.rs').write_text('#[cfg(test)] mod family_tests; mod shape;')
            (src / 'history/family_tests.rs').write_text(in_cx_fn(call))
            (src / 'history/shape.rs').write_text(in_cx_fn(call))
            (src / 'secondary_tests.rs').write_text('mod nested; ' + in_cx_fn(call))
            (src / 'secondary_tests').mkdir()
            (src / 'secondary_tests/nested.rs').write_text(in_cx_fn(call))
            (src / 'undeclared_tests.rs').write_text(in_cx_fn(call))
            # #[path] resolves from the declaring file; a prod declaration keeps a file prod.
            (src / 'support').mkdir()
            (src / 'support/shared.rs').write_text(in_cx_fn(call))
            (src / 'support/only_tests.rs').write_text(in_cx_fn(call))
            (src / 'wire.rs').write_text('#[cfg(test)] #[path = "support/only_tests.rs"] mod only; '
                                        '#[path = "support/shared.rs"] mod shared; '
                                        '#[cfg(test)] #[path = "support/shared.rs"] mod shared_tests;')
            (src / 'lib.rs').write_text('mod history; mod wire; #[cfg(test)] mod secondary_tests; ' + in_cx_fn(call))
            with mock.patch.object(ban, 'ROOT', root):
                prod, test = ban.request_sites()
            digest = dict(expected)
            self.assertEqual(prod, {p: digest for p in [
                'venues/reference/src/history/shape.rs', 'venues/reference/src/lib.rs',
                'venues/reference/src/support/shared.rs', 'venues/reference/src/undeclared_tests.rs']})
            self.assertEqual(test, {p: digest for p in [
                'venues/reference/src/history/family_tests.rs',
                'venues/reference/src/secondary_tests.rs',
                'venues/reference/src/secondary_tests/nested.rs',
                'venues/reference/src/support/only_tests.rs']})


if __name__ == '__main__':
    unittest.main()
