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
        with self.assertRaises(ValueError):
            ban.context_fingerprints('paste::paste! { fn [<keep _ x>](t: u8) {} }')

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



class DataMacroProvenance(unittest.TestCase):
    """Only the extra data-macro row changes; all sender evidence survives."""

    @staticmethod
    def source(body, kind='&Request', generics='', prefix=''):
        return (prefix + ' fn handle' + generics
                + "(cx: &mut Cx<'_, P>, request: " + kind + ') { '
                + 'let escaped = opaque(cx); ' + body + ' }')

    def prod(self, source):
        prod, test = ban.context_fingerprints(source)
        self.assertFalse(test)
        return prod

    def test_all_thirteen_escaped_data_occurrences_have_no_extra_row(self):
        examples = [
            ('matches!(request.kind, Kind::Algo)', 2),
            ('matches!(request.kind, Kind::Normal | Kind::Algo)', 4),
            ('format!("limit=100&page={}", request.page)', 1),
            ('matches!(request.kind, Kind::Metadata)', 1),
            ('matches!(request.kind, Kind::Balance)', 1),
            ('matches!(request.kind, Kind::Normal)', 1),
            ('matches!(request.kind, Kind::Config | Kind::Metadata)', 2),
            ('matches!(request.kind, Kind::Positions)', 1),
        ]
        body = ';'.join(expression for expression, count in examples for _ in range(count)) + ';'
        source = self.source(body)
        self.assertEqual(sum(ban.fingerprints(source).values()), 13)
        self.assertEqual(self.prod(source), {})

    def test_format_references_literals_and_pattern_parentheses(self):
        for expression in ['format!("{} {}", &request.page, 100)',
                           'matches!(request.kind, Kind::One(_) | Kind::Two { field: _ })',
                           'format!("{{cx}} {}", request.page)',
                           'format!("{0:?}", request.page)']:
            with self.subTest(expression=expression):
                self.assertEqual(self.prod(self.source(expression + ';')), {})

    def test_actual_sender_and_wrapper_hashes_are_preserved(self):
        for call in ['cx.request(1)', 'cx.request_streamed(1)',
                     'cx.request_streamed_body(1)', 'cx.sign_http(1)',
                     'cx.done(1)', 'cx.fail(1)', 'cx.r#request(1)',
                     'cx.request::<Pending>(1)', 'Cx::request(cx, 1)',
                     '<Cx as Send>::request(cx, 1)', '(cx).request(1)',
                     'Alias::request(cx, 1)', 'let send = Cx::request']:
            for wrapper in ['matches!({CALL}, Kind::Normal)', 'format!("{}", {CALL})']:
                expression = wrapper.replace('{CALL}', call)
                source = self.source(expression + ';')
                with self.subTest(expression=expression):
                    self.assertEqual(self.prod(source), dict(ban.fingerprints(expression)))

    def test_pure_sibling_never_hides_a_raw_call(self):
        raw = 'matches!(cx.request(1), Kind::Normal);'
        self.assertEqual(self.prod(self.source('matches!(request.kind, Kind::Normal);' + raw)),
                         dict(ban.fingerprints(raw)))

    def test_unknown_nested_callable_and_capability_arguments_stay_frozen(self):
        for expression in ['unknown!(request.kind)', 'path::matches!(request.kind, Kind::Normal)',
                           'format!("{}", request.page())', 'format!("{}", (request.page))',
                           'format!("{}", request[0])', 'format!("{}", request::<T>)',
                           'format!("{}", other!(request.page))',
                           'matches!(request.kind(), Kind::Normal)',
                           'matches!((request.kind), Kind::Normal)',
                           'matches!(request.kind, other!(Kind::Normal))',
                           'matches!(request.kind, Kind::Normal if side_effect())',
                           'format!("{} {}", request.page, cx)',
                           'format!("{cx} {}", request.page)',
                           'format!(r#"{cx} {}"#, request.page)',
                           'format!("{cx:?} {}", request.page)',
                           'format!("{:width$}", request.page)',
                           'format!("{}", selector(request))']:
            with self.subTest(expression=expression):
                self.assertTrue(self.prod(self.source(expression + ';')))

    def test_nonconcrete_or_closure_parameters_cannot_borrow_data_proof(self):
        for kind, generics in [('T', '<T>'), ('impl Trait', ''), ('&dyn Trait', ''),
                               ('_', ''), ('&mut _', ''), ('Vec<T>', '<T>'),
                               ('Self::Request', ''), ('&mut Cx<\'_, P>', '')]:
            source = self.source('matches!(request.kind, Kind::Normal);', kind, generics)
            with self.subTest(kind=kind):
                self.assertTrue(self.prod(source))
        for parameters in ['request', 'request: &Request']:
            source = self.source('each(|' + parameters
                                 + '| { matches!(request.kind, Kind::Normal); });')
            with self.subTest(parameters=parameters):
                self.assertTrue(self.prod(source))

    def test_same_block_and_nonexempt_binders_remain_conservative(self):
        for binding in ['let request = other;', 'let mut request = other;',
                        'let (request, _) = other;', 'let Request { request, .. } = other;',
                        'for request in rows {}', 'if let Some(request) = other {}',
                        'while let Some(request) = other {}',
                        'match other { Some(request) => {}, _ => {} }',
                        'match other { Row { request, .. } => {}, _ => {} }',
                        'each(|request| {});', 'each(|request: &Request| {});',
                        'let alias = request;', 'bind!(request);',
                        'let r#request = other;', 'for r#request in rows {}',
                        'match other { Some(r#request) => {}, _ => {} }',
                        'each(|r#request| {});']:
            for before in [True, False]:
                expression = 'matches!(request.kind, Kind::Normal);'
                source = self.source(binding + expression if before else expression + binding)
                with self.subTest(binding=binding, before=before):
                    self.assertTrue(self.prod(source))

    def test_exact_five_macros_precede_later_nested_queue_error_binding(self):
        macros = ('matches!(request.kind, Kind::Algo);'
                  'matches!(request.kind, Kind::Normal | Kind::Algo);'
                  'matches!(request.kind, Kind::Normal | Kind::Algo);'
                  'format!("limit=100&page={}", request.page);'
                  'matches!(request.kind, Kind::Metadata);')
        tail = ('match result { Ok(_) => {}, Err(pending) => {'
                'let Task::Account(request) = pending.task else { unreachable!() };'
                'matches!(cx.request(1), Kind::Normal); } }')
        raw = dict(ban.fingerprints('matches!(cx.request(1), Kind::Normal);'))
        self.assertEqual(self.prod(self.source(macros + tail)), raw)
        self.assertEqual(self.prod(self.source('matches!(request.kind, Kind::Normal);'
                         'match result { Err(_) => { let request = other; }, _ => {} }')), {})

    def test_narrow_exception_rejects_containment_ambiguity_and_second_binders(self):
        expression = 'matches!(request.kind, Kind::Normal);'
        for body in [
                expression + 'let Task::Account(request) = pending.task else { unreachable!() };',
                'match result { Err(_) => { let request = other;' + expression + ' }, _ => {} }',
                'match result { Err(_) => { let request = other; }, _ => {} }' + expression,
                expression + 'match result { Err(_) => { let request = other; }, _ => {} }'
                + 'let request = second;',
                expression + 'match result { Err(_) => { bind!(request); }, _ => {} }',
                expression + 'match result { Err(_) => { each(|request| {}); }, _ => {} }',
                expression + 'match result { Err(_) => { let make!(request) = other; }, _ => {} }',
                'let Task::Account(request) = pending.task else {' + expression + ' return; };',
                'fn nested() {}' + expression]:
            with self.subTest(body=body):
                self.assertTrue(self.prod(self.source(body)))

    def test_match_guard_references_are_not_patterns_but_guard_binders_refuse(self):
        expression = 'matches!(request.kind, Kind::Normal);'
        self.assertEqual(self.prod(self.source(expression +
                         'match other { Kind::One if request.page > 0 => {}, _ => {} }')), {})
        for arm in ['Some(request) if request.page > 0 => {}',
                    'Kind::One if each(|request| true) => {}',
                    'Kind::One if let Some(request) = other => {}']:
            with self.subTest(arm=arm):
                self.assertTrue(self.prod(self.source(expression + 'match other { ' + arm + ', _ => {} }')))

    def test_visible_macro_shadows_and_import_aliases_are_conservative(self):
        for prefix in ['macro_rules! matches { ($x:expr,$p:pat) => { true } }',
                       'macro_rules! format { ($($x:tt)*) => { 0 } }',
                       'use other::matches;', 'use other::custom as matches;',
                       'use other::{custom as format};', 'use other::*;',
                       'use other::r#matches;', 'use other::r#format;',
                       'macro_rules! r#matches { ($x:expr,$p:pat) => { true } }']:
            source = self.source('matches!(request.kind, Kind::Normal);'
                                 'format!("{}", request.page);', prefix=prefix)
            with self.subTest(prefix=prefix):
                self.assertEqual(sum(self.prod(source).values()), 2)

    PHEMEX = ('let next = actions.iter().filter(|action| matches!(action, Action::ManagedHttp(a) if '
              'matches!(a.as_ref(), safe_cex_engine::request_context::HttpAction::Prepare{request, ..} '
              'if request.method == Method::Delete && request.url.ends_with("orderID=two")))).count();')

    def test_nested_guarded_pattern_bindings_named_like_methods_are_proven(self):
        self.assertEqual(self.prod(self.source(self.PHEMEX)), {})
        self.assertEqual(self.prod(self.source(
            'matches!(step, Step::Send { request, .. } if request.a <= 1 && request.b != 2 && request.c >= 3);')), {})
        self.assertEqual(self.prod(self.source(
            'assert!(ok); let v = vec![1]; matches!(step, Step::Send { request, .. } if request.ready);')), {})

    def test_pattern_proof_keeps_calls_cx_values_and_unbound_names_counted(self):
        for body in [
                'matches!(step, Step::Send { request, .. } if cx.request(1).is_ok());',
                'matches!(step, Step::Send { request, .. } if request.request(1).is_ok());',
                'matches!(step, Step::Send { request, .. } if request(1));',
                'matches!(step, Step::Send { inner, .. } if request.ready);',
                'matches!(step, Step::Send { request: inner, .. } if inner.ready);',
                'matches!(cx, Cx::Send { request, .. } if request.ready);',
                'matches!(cx, request if request.ready);',
                'matches!(step.request, Step::Send);',
                'matches!(step, Step::Send { request, .. } if { cx.done(); true });',
                'matches!(step, Step::Send { request, .. } if (|| request.ready)());',
                'matches!(step, Step::Send { request, .. } if other!(request));',
                'matches!(step, Step::Send { request, .. } if matches!(cx.request(1), Ok(_)));',
                'matches!(step, Step::Send { r#request, .. } if r#request.ready);',
                'matches!(step, Step::Send { request, .. } if request.send(1).is_ok());',
                'matches!(step, Step::Send { request, .. } if request.ready = true);',
                'matches!(step, Step::Send { request, .. } if request.count += 1);',
                'matches!(step, Step::Send { request, .. } if request.done.is_some());',
                'matches!(step, Step::Send { request, .. } if request.0);',
                'matches!(step, Step::Send { request, .. } if consume(request));']:
            with self.subTest(body=body):
                self.assertTrue(self.prod(self.source(body)))

    def test_trusted_prelude_macros_stay_conservative_when_shadowed(self):
        body = 'assert!(ok); matches!(step, Step::Send { request, .. } if request.ready);'
        for prefix in ['macro_rules! assert { ($x:expr) => {} }', 'use other::assert;',
                       'use other::{custom as vec};', 'macro_rules! r#vec { () => {} }',
                       'use other::*;']:
            with self.subTest(prefix=prefix):
                self.assertTrue(self.prod(self.source(body, prefix=prefix)))
        self.assertTrue(self.prod(self.source('custom!(); ' + body)))

    def test_test_partition_and_plain_socket_fingerprints_stay_independent(self):
        pure = self.source('matches!(request.kind, Kind::Normal);')
        raw = self.source('matches!(cx.request(1), Kind::Normal);')
        self.assertEqual(ban.context_fingerprints('#[cfg(test)] ' + pure), ({}, {}))
        prod, test = ban.context_fingerprints('#[cfg(test)] ' + raw)
        self.assertFalse(prod)
        self.assertEqual(test, dict(ban.fingerprints('matches!(cx.request(1), Kind::Normal);')))
        self.assertEqual(sum(ban.fingerprints(pure).values()), 1)


class DataMacroModuleProvenance(unittest.TestCase):
    """Real tracked files/declarations; no inferred external macro exports."""

    def inventory(self, root_source='mod wire; mod account;', target='pub struct Request;',
                  consumer_prefix='use crate::wire::*;', extras=None, consumer='account.rs',
                  body='matches!(request.kind, Kind::Normal);', consumer_suffix='', engine=None,
                  untracked=None):
        with tempfile.TemporaryDirectory() as directory:
            repo = pathlib.Path(directory)
            sdk = repo / 'sdk-rs'
            venue = sdk / 'venues/reference/src'
            venue.mkdir(parents=True)
            files = {'lib.rs': root_source, 'wire.rs': target,
                     consumer: consumer_prefix + DataMacroProvenance.source(body) + consumer_suffix}
            files.update(extras or {})
            for name, source in files.items():
                path = venue / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source)
            for name, source in (engine or {}).items():
                path = sdk / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source)
            for args in [['init'], ['add', 'sdk-rs']]:
                subprocess.run(['git', '-C', str(repo), *args], check=True,
                               capture_output=True, text=True)
            for name, source in (untracked or {}).items():
                (sdk / name).write_text(source)
            with mock.patch.object(ban, 'ROOT', sdk):
                prod, test = ban.request_sites()
            self.assertFalse(test)
            return prod

    def test_single_hop_private_imports_do_not_reexport_macros_to_sibling(self):
        for imports in ['', 'use external::Thing;', 'use external::*;',
                        'use external::{Thing, Other};']:
            with self.subTest(imports=imports):
                self.assertEqual(self.inventory(target=imports + 'pub struct Request;'), {})

    def test_every_visible_reexport_and_macro_source_stays_conservative(self):
        for target in ['pub use external::*;', 'pub(crate) use external::*;',
                       'pub(super) use external::*;', 'pub(self) use external::*;',
                       'pub(in crate) use external::*;', 'pub use external::matches;',
                       'macro_rules! custom { () => { 0 } }', 'macro custom() {}',
                       '#[macro_use] extern crate external;', '#[macro_export] fn custom() {}',
                       'mod nested;', 'unknown_items!();']:
            with self.subTest(target=target):
                self.assertTrue(self.inventory(target=target))

    def test_missing_ambiguous_path_conditional_and_grouped_targets_stay_conservative(self):
        for root, prefix, extras in [
                ('mod account;', 'use crate::missing::*;', {}),
                ('mod wire; mod account;', 'use crate::wire::*;', {'wire/mod.rs': ''}),
                ('#[path="wire.rs"] mod wire; mod account;', 'use crate::wire::*;', {}),
                ('#[cfg(feature="x")] mod wire; mod account;', 'use crate::wire::*;', {}),
                ('mod wire; mod account;', 'use crate::{wire::*};', {}),
                ('mod wire; mod account;', 'use super::wire::*;', {}),
                ('mod wire; mod account;', 'use external::*;', {}),
                ('mod wire; mod account;', 'use crate::wire::*; use external::*;', {})]:
            with self.subTest(root=root, prefix=prefix):
                self.assertTrue(self.inventory(root_source=root, consumer_prefix=prefix, extras=extras))

    def test_test_only_target_modules_do_not_block_the_crate_glob_proof(self):
        self.assertEqual(self.inventory(target='pub struct Request; #[cfg(test)] mod tests { use super::*; }'), {})
        for target in ['pub struct Request; #[cfg(test)] pub mod tests {}',
                       'pub struct Request; #[cfg(test)] pub(crate) mod tests {}',
                       'pub struct Request; #[cfg(not(test))] mod nested {}',
                       'pub struct Request; mod nested {}',
                       'pub struct Request; #[cfg(test)] mod tests { macro_rules! matches { () => {} } }']:
            with self.subTest(target=target):
                self.assertTrue(self.inventory(target=target))

    ENGINE = {'Cargo.toml': '[workspace.dependencies]\nsafe-cex-engine = { path = "engine" }\n',
              'venues/reference/Cargo.toml': '[dependencies]\nsafe-cex-engine = { workspace = true }\n',
              'engine/src/lib.rs': 'pub mod model;\n',
              'engine/src/model.rs': 'pub enum Kind { Normal } pub(crate) mod inner {} '
                                     '#[cfg(test)] mod tests { use super::*; }'}

    def engine_inventory(self, prefix='use safe_cex_engine::model::*;',
                         body='matches!(request.kind, Kind::Normal);', **changes):
        return self.inventory(consumer_prefix=prefix, body=body, engine={**self.ENGINE, **changes})

    def test_a_workspace_engine_module_glob_without_reexports_is_proven(self):
        self.assertEqual(self.engine_inventory(), {})
        self.assertEqual(self.engine_inventory(**{'venues/reference/Cargo.toml':
            '[dependencies]\nsafe-cex-engine = { workspace = true, features = ["x"] }\n'}), {})
        # The Bitget false positive: a pattern match over a plain local named `request`.
        self.assertEqual(self.engine_inventory(
            prefix='use crate::wire::*; use safe_cex_engine::model::*;',
            body='matches!(request, Request::Probe);'), {})

    def test_engine_module_globs_that_could_carry_a_macro_stay_conservative(self):
        cases = {
            'pub use': {'engine/src/model.rs': 'pub use std::matches; pub enum Kind { Normal }'},
            'restricted use': {'engine/src/model.rs': 'pub(crate) use x::*; pub enum Kind { Normal }'},
            'macro definition': {'engine/src/model.rs': 'macro_rules! matches { ($a:expr,$b:pat) => { true } }'},
            'exported macro': {'engine/src/model.rs': '#[macro_export] macro_rules! custom { () => {} }'},
            'item macro': {'engine/src/model.rs': 'items!(); pub enum Kind { Normal }'},
            'conditional module': {'engine/src/lib.rs': '#[cfg(feature = "x")] pub mod model;\n'},
            'path module': {'engine/src/lib.rs': '#[path = "model.rs"] pub mod model;\n'},
            'missing module': {'engine/src/lib.rs': 'pub mod other;\n'},
            'lib macro': {'engine/src/lib.rs': 'pub mod model; macro_rules! m { () => {} }\n'},
            'renamed dependency': {'venues/reference/Cargo.toml':
                '[dependencies]\nsafe-cex-engine = { package = "other", path = "../other" }\n'},
            'elsewhere workspace': {'Cargo.toml': '[workspace.dependencies]\nsafe-cex-engine = { path = "fork" }\n'},
            'no dependency': {'venues/reference/Cargo.toml': '[dependencies]\n'},
        }
        for name, changes in cases.items():
            with self.subTest(name):
                self.assertTrue(self.engine_inventory(**changes))
        for prefix in ['use safe_cex_engine::*;', 'use safe_cex_engine::model::inner::*;',
                       'use safe_cex_engine::{model::*};', 'use safe_cex_engine::model::*; use other::*;']:
            with self.subTest(prefix=prefix):
                self.assertTrue(self.engine_inventory(prefix=prefix))
        engine = dict(self.ENGINE)
        model = engine.pop('engine/src/model.rs')
        with self.subTest('untracked module'):
            self.assertTrue(self.inventory(consumer_prefix='use safe_cex_engine::model::*;', engine=engine,
                                           untracked={'engine/src/model.rs': model}))

    def test_engine_glob_proof_keeps_legacy_calls_and_shadows_counted(self):
        for body in ['matches!(cx.request(1), Kind::Normal);', 'cx.request(1);']:
            with self.subTest(body=body):
                self.assertTrue(self.engine_inventory(body=body))
        self.assertTrue(self.engine_inventory(prefix='use safe_cex_engine::model::*; use other::matches;'))

    def test_super_globs_prove_each_parent_namespace(self):
        body = 'matches!(step, Step::Send { request, .. } if request.ready);'
        self.assertEqual(self.inventory(consumer_prefix='use super::*;', body=body), {})
        self.assertEqual(self.inventory(root_source='mod wire; mod account; use crate::wire::*;',
                                        consumer_prefix='use super::*;', body=body), {})
        self.assertEqual(self.inventory(consumer_prefix='', body=body,
                                        consumer_suffix=' #[cfg(test)] mod tests { use super::*; '
                                        "fn t(cx: &mut Cx<'_, P>) { let e = opaque(cx); assert!(true); "
                                        'matches!(s, S::A { request } if request.b); } }'), {})
        self.assertTrue(self.inventory(consumer_prefix='', body=body,
                                       consumer_suffix=' mod tests { use other::*; '
                                       "fn t(cx: &mut Cx<'_, P>) { let e = opaque(cx); "
                                       'matches!(s, S::A { request } if request.b); } }'))
        for root, prefix, suffix in [
                ('mod wire; mod account; use other::matches;', 'use super::*;', ''),
                ('mod wire; mod account; use other::*;', 'use super::*;', ''),
                ('mod wire; mod account; use other::{x as assert};', 'use super::*;', ''),
                ('mod wire; mod account; use crate::wire::*;', 'use super::*;', ''),
                ('mod wire; mod account;', 'use super::super::*;', ''),
                ('mod wire; mod account;', '', " fn g(cx: &mut Cx<'_, P>) { let e = opaque(cx); use super::*; "
                 'matches!(s, S::A { request } if request.b); }'),
                ('mod wire; mod account; use other::*;', '',
                 " mod inner { use super::super::*; fn t(cx: &mut Cx<'_, P>) { let e = opaque(cx); "
                 'matches!(s, S::A { request } if request.b); } }'),
                ('mod wire; mod account;', 'use other::*;',
                 " mod inner { use super::*; fn t(cx: &mut Cx<'_, P>) { let e = opaque(cx); "
                 'matches!(s, S::A { request } if request.b); } }')]:
            with self.subTest(root=root, prefix=prefix, suffix=suffix):
                target = 'pub struct Request; mod nested {}' if 'crate::wire' in root else 'pub struct Request;'
                found = self.inventory(root_source=root, consumer_prefix=prefix,
                                       body='let ok = true;' if suffix else body,
                                       consumer_suffix=suffix, target=target)
                self.assertTrue(found)
        # The crate root has no parent namespace.
        self.assertTrue(self.inventory(root_source='use super::*; mod wire;', consumer='lib.rs',
                                       consumer_prefix='mod wire; use super::*;', body=body))

    def test_descendant_cannot_use_private_target_import_exception(self):
        self.assertTrue(self.inventory(root_source='mod wire;', target='use external::*; mod account;',
                                       consumer='wire/account.rs'))

    def test_ancestor_macro_shadow_stays_conservative(self):
        for prefix in ['macro_rules! matches { ($x:expr,$p:pat) => { true } }',
                       '#[macro_use] extern crate external;', '#[macro_use] mod sibling;']:
            with self.subTest(prefix=prefix):
                self.assertTrue(self.inventory(root_source=prefix + 'mod wire; mod account;'))


    def test_out_of_scope_globs_and_ordinary_ancestor_imports_do_not_reach_child(self):
        for prefix in ['use external::custom as matches;', 'use wire::*;']:
            with self.subTest(prefix=prefix):
                self.assertEqual(self.inventory(root_source=prefix + 'mod wire; mod account;'), {})
        for prefix in ['fn elsewhere() { use external::*; }',
                       '#[cfg(test)] mod tests { use super::*; const TEXT: &str = r#"} {"#; }']:
            with self.subTest(prefix=prefix):
                self.assertEqual(self.inventory(consumer_prefix=prefix + 'use crate::wire::*;'), {})

    def test_enclosing_block_imports_are_visible_even_after_invocation(self):
        for body in ['use external::*; matches!(request.kind, Kind::Normal);',
                     'matches!(request.kind, Kind::Normal); use external::*;',
                     '{ use external::*; matches!(request.kind, Kind::Normal); }',
                     'use super::*; matches!(request.kind, Kind::Normal);']:
            with self.subTest(body=body):
                self.assertTrue(ban.context_fingerprints(DataMacroProvenance.source(body))[0])


    def test_tokenized_literal_braces_cannot_hide_an_enclosing_glob(self):
        source = ('#[cfg(test)] mod tests { use external::*; const TEXT: &str = r#"} {"#; '
                  + DataMacroProvenance.source('matches!(request.kind, Kind::Normal);') + ' }')
        prod, test = ban.context_fingerprints(source)
        self.assertFalse(prod)
        self.assertTrue(test)


    def test_opaque_item_generators_without_data_arguments_stay_conservative(self):
        for generator in ['install!();', 'install_shadow!();', 'install_import!();',
                          'external::install!();', 'install! {}']:
            for local in [False, True]:
                expression = 'matches!(request.kind, Kind::Normal);'
                prefix = 'use external::{install, install_shadow, install_import};'
                source = DataMacroProvenance.source(
                    generator + expression if local else expression,
                    prefix=prefix + ('' if local else generator))
                with self.subTest(generator=generator, local=local, boundary='source'):
                    self.assertTrue(ban.context_fingerprints(source)[0])
                with self.subTest(generator=generator, local=local, boundary='tracked'):
                    self.assertTrue(self.inventory(consumer_prefix=prefix +
                        ('' if local else generator) + 'use crate::wire::*;',
                        body=generator + expression if local else expression))

    def test_generator_after_invocation_and_unknown_expression_position_stay_conservative(self):
        expression = 'matches!(request.kind, Kind::Normal);'
        for generator in ['install!();', 'let value = install!();']:
            with self.subTest(generator=generator, boundary='source'):
                self.assertTrue(ban.context_fingerprints(
                    DataMacroProvenance.source(expression + generator))[0])
            with self.subTest(generator=generator, boundary='tracked'):
                self.assertTrue(self.inventory(body=expression + generator))

    def test_disjoint_generator_scopes_do_not_broaden_visibility(self):
        expression = 'matches!(request.kind, Kind::Normal);'
        for prefix in ['fn elsewhere() { install!(); }',
                       '#[cfg(test)] mod tests { install!(); }']:
            with self.subTest(prefix=prefix):
                self.assertEqual(self.inventory(consumer_prefix=prefix + 'use crate::wire::*;'), {})
        self.assertEqual(self.inventory(body=expression +
                         'match result { Err(_) => { install!(); }, _ => {} }'), {})


    def test_generator_refusal_never_changes_actual_sender_or_wrapper_hashes(self):
        raw = 'matches!(cx.request(1), Kind::Normal);'
        for prefix, body in [('install!();', raw), ('', 'install!();' + raw)]:
            with self.subTest(prefix=prefix):
                expected = {'venues/reference/src/account.rs': dict(ban.fingerprints(raw))}
                self.assertEqual(self.inventory(consumer_prefix=prefix + 'use crate::wire::*;', body=body),
                                 expected)


    def test_inline_ancestor_generator_cannot_install_a_textual_macro(self):
        for prefix in ['install!();', 'use external::install as format; format!();']:
            source = prefix + 'mod child {' + DataMacroProvenance.source(
                'matches!(request.kind, Kind::Normal);') + '}'
            with self.subTest(prefix=prefix, boundary='source'):
                self.assertTrue(ban.context_fingerprints(source)[0])
            with self.subTest(prefix=prefix, boundary='tracked'):
                self.assertTrue(self.inventory(consumer_prefix=prefix + 'mod child {',
                                               consumer_suffix='}'))
        ordinary = 'use external::install as format; mod child {'
        self.assertEqual(self.inventory(consumer_prefix=ordinary, consumer_suffix='}'), {})


if __name__ == '__main__':
    unittest.main()
