#!/usr/bin/env python3
"""Shrinking inventory of credential/nonce choices outside the engine core.

Venues declare a CredentialRole and sign through Slots::select; hosts route on
declared intent. Every remaining venue-local selection or nonce site, and every
host GET/header egress inference, is counted per file and term. Adoption PRs
prune their counts; no PR may grow one. Conservative lexical ban, not name
resolution: a rename reads as a prune, so reviewers confirm every removed count
is a real adoption. See docs/plans/RUST_CREDENTIAL_SELECTION.md (C4).
"""
import collections
import importlib.util
import json
import os
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
# Engine and workspace-macro sources the crate-wide proof reads; never a fixture root.
SDK = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = 'scripts/credential-legacy-sites.json'

_spec = importlib.util.spec_from_file_location(
    'request_tokens', pathlib.Path(__file__).with_name('check-request-context.py'))
_request = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_request)
tokens = _request.tokens

# Venue-local key choice, evidence tagging and nonce allocation.
VENUE_TERMS = {'read_credentials', 'ReadCredentials', 'tag_account_read', 'ethereum_agent', 'NONCES', 'reserve_kraken_legacy'}
NONCE_CALL = 'nonce::next'
HOST_TERM = 'Method::Get'
NATIVE_HOST = 'hosts/native/src/routing.rs'
TS_HOST = '../packages/safe-cex/src/transport/engine.ts'
# TS egress inference: method sniffing and the hard-coded venue list.
TS_PATTERNS = {
    'GET': re.compile(r"""toUpperCase\(\)\s*===\s*['"]GET['"]"""),
    'venue-list': re.compile(r"""\[\s*['"]binance['"][^\]]*['"]asterdex['"]\s*\]"""),
}


def strip_test_modules(stream):
    """Drop only `#[cfg(test)] mod name;` and `#[cfg(test)] mod name { ... }`."""
    attribute = ['#', '[', 'cfg', '(', 'test', ')', ']']
    kept = []
    i = 0
    while i < len(stream):
        j = i + len(attribute)
        if stream[i:j] == attribute:
            k = j + (stream[j:j + 1] == ['pub'])
            if stream[k:k + 1] == ['mod'] and k + 2 < len(stream):
                if stream[k + 2] == ';':
                    i = k + 3
                    continue
                if stream[k + 2] == '{':
                    depth, end = 0, k + 2
                    while end < len(stream):
                        depth += (stream[end] == '{') - (stream[end] == '}')
                        end += 1
                        if not depth:
                            break
                    if depth:
                        raise ValueError('unbalanced test module')
                    i = end
                    continue
        kept.append(stream[i])
        i += 1
    return kept


def evasions(stream):
    """Aliases and imports that would hide later sites from the counts."""
    found = []
    for i, token in enumerate(stream):
        if token == 'use':
            end = stream.index(';', i) if ';' in stream[i:] else len(stream)
            if 'nonce' in stream[i:end]:
                found.append('nonce import')
        if token == 'as' and i and stream[i - 1].removeprefix('r#') in VENUE_TERMS | {'nonce', 'next'}:
            found.append(f'renamed {stream[i - 1]}')
    return found


def rust_counts(source, native_host=False):
    stream = strip_test_modules(tokens(source))
    found = collections.Counter()
    for i, token in enumerate(stream):
        name = token.removeprefix('r#')
        if name in VENUE_TERMS:
            found[name] += 1
        elif name == 'next' and stream[i - 2:i] == ['nonce', '::']:
            found[NONCE_CALL] += 1
        elif native_host and name == 'Get' and stream[i - 2:i] == ['Method', '::']:
            found[HOST_TERM] += 1
    return found, evasions(stream)


_scope_spec = importlib.util.spec_from_file_location(
    'rust_scope', pathlib.Path(__file__).with_name('rust_scope.py'))
rust_scope = importlib.util.module_from_spec(_scope_spec)
_scope_spec.loader.exec_module(rust_scope)

PROVIDER_IDENTITY = ['safe_cex_engine', '::', 'signing', '::', 'Identity']
ROOT_NAMES = {'safe_cex_engine', 'Option'}
# Tokens that may follow a field read; none starts a call (`(`, `::` turbofish).
FIELD_FOLLOW = {'.', ')', ',', ';', '}', ']', '?', '{', '&', '|', '=', '!', '<', '>', '+', '-', '*', '/',
                '%', '^', 'as', 'else'}


def provider_type(stream, start, end):
    """Whether stream[start:end] is exactly the engine signing Identity. The crate-wide
    proof separately shows `safe_cex_engine` and `Option` are unshadowed."""
    declared = stream[start:end]
    return declared == PROVIDER_IDENTITY or (
        declared == ['Identity'] and rust_scope.resolve(
            stream, 'Identity', start, ['safe_cex_engine', 'signing', 'Identity']) == 'provider')


def opener(stream, index):
    """Index of the innermost unmatched `{` or `(` before `index`, else -1."""
    depth = 0
    for i in range(index - 1, -1, -1):
        if stream[i] in {'}', ')'}:
            depth += 1
        elif stream[i] in {'{', '('}:
            if not depth:
                return i
            depth -= 1
    return -1


def angle_end(stream, start):
    """Index just past the `>` closing the `<` at `start`."""
    depth = 0
    for k in range(start, len(stream)):
        depth += (stream[k] == '<') - (stream[k] == '>')
        if not depth:
            return k + 1
    raise ValueError('unbalanced generics')


def option_of_provider(stream, at):
    """End index when stream[at:] is `Option<Identity>` (provider-typed), else None."""
    if stream[at:at + 2] != ['Option', '<']:
        return None
    end = angle_end(stream, at + 1)
    return end if provider_type(stream, at + 2, end - 1) else None


def struct_fields(stream):
    """{name: (keyword index, {field: type tokens})} for module-level braced structs."""
    depth = rust_scope.brace_depths(stream)
    found = {}
    for i, token in enumerate(stream):
        if token != 'struct' or depth[i] != 0 or not rust_scope.IDENT.fullmatch(stream[i + 1]):
            continue
        body = i + 2
        if stream[body:body + 1] == ['<']:
            body = angle_end(stream, body)
        if stream[body:body + 1] != ['{']:
            continue
        fields, k, close = {}, body + 1, rust_scope.group_close(stream, body) - 1
        while k < close:
            if stream[k] == '#':
                k = rust_scope.group_close(stream, k + 1)
                continue
            if stream[k] == 'pub':
                k += 1
                if stream[k:k + 1] == ['(']:
                    k = rust_scope.group_close(stream, k)
            name, k = stream[k], k + 1
            if stream[k:k + 1] != [':']:
                fields = None
                break
            typed, angle, k = [], 0, k + 1
            while k < close and not (angle == 0 and stream[k] == ','):
                if stream[k] in {'(', '['}:
                    stop = rust_scope.group_close(stream, k)
                    typed += stream[k:stop]
                    k = stop
                    continue
                angle += (stream[k] == '<') - (stream[k] == '>')
                typed.append(stream[k])
                k += 1
            fields[name] = (typed, k - len(typed))
            k += 1
        if fields is not None:
            found[stream[i + 1]] = (i, fields)
    return found


def impl_type(stream, index):
    """Self type name of the innermost `impl` block containing `index`, else None."""
    best = None
    for j, token in enumerate(stream[:index]):
        if token != 'impl' or (j and stream[j - 1] in {'(', ',', '->', ':', '&', '<', 'dyn', '='}):
            continue
        k = j + 1
        if stream[k:k + 1] == ['<']:
            k = angle_end(stream, k)
        head = []
        while k < len(stream) and stream[k] not in {'{', 'where', ';'}:
            if stream[k] == '<':
                stop = angle_end(stream, k)
                head.append(stream[k:stop])
                k = stop
                continue
            head.append(stream[k])
            k += 1
        while k < len(stream) and stream[k] != '{':
            k += 1
        if k >= len(stream) or not k < index < rust_scope.group_close(stream, k):
            continue
        if 'for' in head:
            head = head[head.index('for') + 1:]
        names = [part for part in head if isinstance(part, str)]
        if len(names) == 1 and rust_scope.IDENT.fullmatch(names[0]) and len(head) <= 2:
            best = names[0]
    return best


def enclosing_fn(stream, index):
    """(parameters {name: type tokens}, body start, body end) of the innermost fn, else None."""
    best = None
    for j, token in enumerate(stream[:index]):
        if token != 'fn' or not rust_scope.IDENT.fullmatch(stream[j + 1:j + 2][0] if j + 1 < len(stream) else ''):
            continue
        k = j + 2
        if stream[k:k + 1] == ['<']:
            k = angle_end(stream, k)
        if stream[k:k + 1] != ['(']:
            continue
        close = rust_scope.group_close(stream, k)
        body = close
        while body < len(stream) and stream[body] not in {'{', ';'}:
            body += 1
        if body >= len(stream) or stream[body] != '{':
            continue
        end = rust_scope.group_close(stream, body)
        if body < index < end and (best is None or body > best[1]):
            parameters, part, depth = {}, [], 0
            for t in stream[k + 1:close - 1] + [',']:
                depth += (t in {'(', '[', '<'}) - (t in {')', ']', '>'})
                if t == ',' and depth == 0:
                    if part[:1] == ['mut']:
                        part = part[1:]
                    if len(part) >= 3 and part[1] == ':':
                        parameters[part[0]] = part[2:]
                    part = []
                else:
                    part.append(t)
            best = (parameters, body, end)
    return best


class CrateTypes:
    """Holder structs whose `read_credentials` field is the provider Identity, and the
    impl/field/parameter typing that ties each member read to one of them."""

    def __init__(self, streams):
        self.streams = streams
        self.structs, duplicate = {}, set()
        for path, stream in streams.items():
            for name, (at, fields) in struct_fields(stream).items():
                if name in self.structs:
                    duplicate.add(name)
                self.structs[name] = (path, at, fields)
        for name in duplicate:
            del self.structs[name]
        self.holders = set()
        for name, (path, at, fields) in self.structs.items():
            if 'read_credentials' in fields:
                typed, start = fields['read_credentials']
                stream = self.streams[path]
                if option_of_provider(stream, start) == start + len(typed):
                    self.holders.add(name)
        self.used = set()

    def holder(self, name):
        if name in self.holders:
            self.used.add(name)
            return True
        return False

    def field_holder(self, owner, field):
        if owner not in self.structs:
            return False
        typed = self.structs[owner][2].get(field, ([], 0))[0]
        if len(typed) == 1 and self.holder(typed[0]):
            self.used.add(owner)
            return True
        return False

    def occurrence(self, stream, i):
        """Whether the `read_credentials` token at `i` provably holds or reads a holder."""
        if stream[i - 1] == '.':
            # A field read: never a method call, a turbofish, or a call of the
            # parenthesized value.
            j = i + 1
            while stream[j:j + 1] == [')']:
                j += 1
            if stream[i + 1] not in FIELD_FOLLOW or stream[j:j + 1] in (['('], ['::']):
                return False
            if stream[i - 2] == 'self' and stream[i - 3] not in {'.', '::'}:
                owner = impl_type(stream, i)
                return owner is not None and self.holder(owner) and self.used.add(owner) is None
            if stream[i - 4:i - 2] == ['self', '.'] and stream[i - 5] not in {'.', '::'}:
                owner = impl_type(stream, i)
                return owner is not None and self.field_holder(owner, stream[i - 2])
            receiver = stream[i - 2]
            if not rust_scope.IDENT.fullmatch(receiver) or stream[i - 3] in {'.', '::'}:
                return False
            scope = enclosing_fn(stream, i)
            if scope is None or receiver not in scope[0]:
                return False
            typed = scope[0][receiver]
            if typed[:1] == ['&']:
                typed = typed[1 + (typed[1:2] == ['mut']):]
            if len(typed) != 1 or not self.holder(typed[0]):
                return False
            # Bindings scope forward, so only earlier tokens can rebind the parameter:
            # each must be a member read or the type-preserving `let [mut] x = x;`.
            def kept(k):
                if stream[k + 1] == '.':
                    return True
                borrowed = stream[k - 1] == '&' or stream[k - 2:k] == ['&', 'mut']
                if borrowed and stream[k + 1] in {')', ',', ';'}:
                    return True  # A borrow is an expression, never a binding position.
                rebind = stream[k - 1] == 'let' or stream[k - 2:k] == ['let', 'mut']
                same = stream[k + 1:k + 4] == ['=', receiver, ';']
                moved = stream[k - 1] == '=' and stream[k + 1] == ';' and (
                    stream[k - 3] == 'let' or stream[k - 4:k - 2] == ['let', 'mut']) and stream[k - 2] == receiver
                return (rebind and same) or moved
            return all(kept(k) for k in range(scope[1], i)
                       if stream[k] == receiver and stream[k - 1] not in {'.', '::'})
        if stream[i - 1] not in {'{', ',', ')', '(', 'pub'}:
            return False
        open_at = opener(stream, i)
        if open_at < 0:
            return False
        if stream[open_at] == '{':
            if stream[open_at - 2:open_at - 1] in (['struct'], ['union']):
                return self.holder(stream[open_at - 1])  # The holder's own declaration.
            if stream[i + 1:i + 2] not in ([':'], [','], ['}']):
                return False
            head = stream[open_at - 1]
            if stream[open_at - 2:open_at - 1] in (['::'], ['enum'], ['impl'], ['for']):
                return False
            if head == 'Self':
                owner = impl_type(stream, i)
                return owner is not None and self.holder(owner)
            return self.holder(head)
        # A function parameter declared as the provider Identity.
        head = stream[open_at - 2:open_at]
        named_fn = head[:1] == ['fn'] or (stream[open_at - 1:open_at] == ['>'] and 'fn' in stream[max(0, open_at - 40):open_at])
        end = option_of_provider(stream, i + 2) if stream[i + 1:i + 2] == [':'] else None
        return bool(named_fn and end and stream[end:end + 1] in ([','], [')']))


def identity_crates(files):
    """Crates in which every `read_credentials` token is proven to hold or read a
    provider holder, at least one holder exists, and the crate-wide proof shows the
    holder, impl and root names unshadowed with every generator and glob closed."""
    by_crate = collections.defaultdict(dict)
    for _, path, native in files:
        if not native:
            by_crate[path.relative_to(ROOT / 'venues').parts[0]][path] = rust_scope.strip_cfg_test(tokens(path.read_text()))
    adopted = set()
    for crate, streams in by_crate.items():
        if not any('read_credentials' in stream for stream in streams.values()):
            continue
        types = CrateTypes(streams)
        proven = bool(types.holders) and all(
            token == 'read_credentials' and types.occurrence(stream, i)
            for stream in streams.values() for i, token in enumerate(stream)
            if token.removeprefix('r#') == 'read_credentials')
        if not proven or not types.used & types.holders:
            continue
        watched = ROOT_NAMES | types.used
        definitions = {name: (types.structs[name][0], types.structs[name][1]) for name in types.used}
        allowed = lambda name, path: name in types.used and path[:1] in (('crate',), ('super',), ('self',)) \
            and path[-1] == name
        if not rust_scope.crate_refusals(SDK, streams, watched, allowed, definitions, production=True):
            adopted.add(crate)
    return adopted


def ts_counts(source):
    return collections.Counter(term for line in source.splitlines()
                               for term, pattern in TS_PATTERNS.items() if pattern.search(line))


def production(path):
    return not (path.name == 'tests.rs' or path.name.endswith(('_tests.rs', '_test.rs'))
                or 'tests' in path.parts)


def sites():
    result, failures = {}, []
    files = [(str(p.relative_to(ROOT)), p, False)
             for p in sorted((ROOT / 'venues').glob('*/src/**/*.rs')) if production(p)]
    adopted = identity_crates(files)
    for name, path, native in [*files, (NATIVE_HOST, ROOT / NATIVE_HOST, True)]:
        counts, hidden = rust_counts(path.read_text(), native_host=native)
        if not native and path.relative_to(ROOT / 'venues').parts[0] in adopted:
            counts.pop('read_credentials', None)
        failures += [f'{name}: {reason}' for reason in hidden]
        if counts:
            result[name] = dict(counts)
    if counts := ts_counts((ROOT / TS_HOST).read_text()):
        result[TS_HOST] = dict(counts)
    return result, failures


def manifest(text):
    value = json.loads(text)
    if not isinstance(value, dict):
        raise ValueError('manifest must be a path/count map')
    terms = VENUE_TERMS | {NONCE_CALL, HOST_TERM, *TS_PATTERNS}
    for path, counts in value.items():
        if path not in {NATIVE_HOST, TS_HOST} and not re.fullmatch(r'venues/[A-Za-z0-9_-]+/src/.+\.rs', path):
            raise ValueError('invalid manifest path')
        if not isinstance(counts, dict) or not counts or any(
                term not in terms or type(count) is not int or count <= 0 for term, count in counts.items()):
            raise ValueError('invalid manifest term/count')
    return value


def check(current, allowed, previous):
    failures = []
    if current != allowed:
        failures.append('current sites must exactly match the inventory (prune removals; no new sites)')
    for label, baseline in previous:
        for path, counts in allowed.items():
            bound = baseline.get(path, {})
            if any(count > bound.get(term, 0) for term, count in counts.items()):
                failures.append(f'{path}: inventory grew against {label}')
    return failures


def git(*args):
    return subprocess.run(['git', '-C', str(ROOT), *args], text=True, capture_output=True, check=True).stdout


def previous_manifests():
    base = os.environ.get('GITHUB_BASE_REF') or 'feat/rust-rewrite'
    ref = f'refs/remotes/origin/{base}'
    relative = git('rev-parse', '--show-prefix').strip() + MANIFEST
    git('rev-parse', '--verify', f'{ref}^{{commit}}')
    if relative in git('ls-tree', '--full-name', '--name-only', ref, '--', MANIFEST).splitlines():
        return [(ref, manifest(git('show', f'{ref}:{relative}')))]
    return []


def main():
    try:
        current, failures = sites()
        failures += check(current, manifest((ROOT / MANIFEST).read_text()), previous_manifests())
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        failures = [f'cannot verify credential inventory: {type(error).__name__}']
    for failure in failures:
        print('BANNED: credential selection: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    if os.environ.get('CREDENTIAL_INVENTORY_WRITE') == '1':
        (ROOT / MANIFEST).write_text(json.dumps(sites()[0], indent=2, sort_keys=True) + '\n')
    raise SystemExit(main())
