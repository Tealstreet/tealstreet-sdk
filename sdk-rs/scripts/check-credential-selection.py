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


PROVIDER_IDENTITY = ['safe_cex_engine', '::', 'signing', '::', 'Identity']


def provider_import(stream):
    """Whether the file imports the provider `Identity` from `safe_cex_engine::signing`."""
    for i, token in enumerate(stream):
        if token == 'use' and stream[i + 1:i + 5] == PROVIDER_IDENTITY[:4]:
            end = stream.index(';', i)
            body = stream[i + 5:end]
            if body == ['Identity'] or (body[:1] == ['{'] and 'Identity' in body
                                        and 'as' not in body[body.index('Identity') - 1:body.index('Identity') + 2]):
                return True
    return False


def read_credential_types(stream):
    """Type tokens of every `read_credentials: Option<...>` declaration."""
    found = []
    for i, token in enumerate(stream):
        if token == 'read_credentials' and stream[i + 1:i + 4] == [':', 'Option', '<']:
            depth, end = 1, i + 4
            while end < len(stream) and depth:
                depth += (stream[end] == '<') - (stream[end] == '>')
                end += 1
            found.append(stream[i + 4:end - 1])
    return found


def identity_crates(files):
    """Crates whose every `read_credentials` declaration is the provider Identity:
    its full path, or a bare `Identity` in a file importing that path."""
    found, provider = collections.defaultdict(list), collections.defaultdict(list)
    for _, path, native in files:
        if not native:
            crate = path.relative_to(ROOT / 'venues').parts[0]
            stream = strip_test_modules(tokens(path.read_text()))
            imported = provider_import(stream)
            for declared in read_credential_types(stream):
                found[crate].append(declared == PROVIDER_IDENTITY
                                    or (declared == ['Identity'] and imported))
    return {crate for crate, typed in found.items() if typed and all(typed)}


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
