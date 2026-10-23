#!/usr/bin/env python3
"""Exact, shrinking inventory of credential/nonce choices outside the engine core.

Venues declare a CredentialRole and sign through Slots::select; hosts route on
declared intent. Every remaining venue-local selection or nonce site, and every
host GET/header egress inference, is frozen here. Adoption PRs prune their rows;
no PR may add one. Conservative lexical ban, not Rust name resolution.
See docs/plans/RUST_CREDENTIAL_SELECTION.md (C4).
"""
import collections
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
REPO = ROOT.parent
MANIFEST = 'scripts/credential-legacy-sites.json'

_spec = importlib.util.spec_from_file_location(
    'request_tokens', pathlib.Path(__file__).with_name('check-request-context.py'))
_request = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_request)
tokens = _request.tokens

# Venue-local key choice, evidence tagging and nonce allocation.
VENUE_TERMS = {'read_credentials', 'ReadCredentials', 'tag_account_read', 'ethereum_agent', 'NONCES'}
WINDOW = 6
NATIVE_HOST = 'hosts/native/src/routing.rs'
TS_HOST = '../packages/safe-cex/src/transport/engine.ts'
# TS egress inference: method sniffing and the hard-coded venue list.
TS_PATTERNS = [re.compile(r"toUpperCase\(\)\s*===\s*'GET'"),
               re.compile(r"\['binance',\s*'binancepm',\s*'binanceinverse',\s*'asterdex'\]")]


def digest(parts):
    payload = json.dumps(parts, ensure_ascii=False, separators=(',', ':'))
    return hashlib.sha256(payload.encode()).hexdigest()


def rust_fingerprints(source, native_host=False):
    stream = tokens(source)
    found = collections.Counter()
    for i, token in enumerate(stream):
        name = token.removeprefix('r#')
        venue_site = name in VENUE_TERMS or (
            name == 'next' and stream[i - 2:i] == ['nonce', '::'])
        host_site = native_host and name == 'Get' and stream[i - 2:i] == ['Method', '::']
        if venue_site or host_site:
            found[digest(stream[max(0, i - WINDOW):i + WINDOW + 1])] += 1
    return found


def without_test_module(source):
    """Inline test modules close each file by convention; they are not production."""
    match = re.search(r'^#\[cfg\(test\)\]\s*\n\s*mod\s', source, re.M)
    return source[:match.start()] if match else source


def ts_fingerprints(source):
    found = collections.Counter()
    for line in source.splitlines():
        if any(pattern.search(line) for pattern in TS_PATTERNS):
            found[digest(['ts', ' '.join(line.split())])] += 1
    return found


def production(path):
    name = path.name
    return not (name.endswith('tests.rs') or name.endswith('_test.rs') or 'tests' in path.parts)


def sites():
    result = {}
    for path in sorted((ROOT / 'venues').glob('*/src/**/*.rs')):
        if production(path) and (counts := rust_fingerprints(without_test_module(path.read_text()))):
            result[str(path.relative_to(ROOT))] = dict(counts)
    if counts := rust_fingerprints(without_test_module((ROOT / NATIVE_HOST).read_text()), native_host=True):
        result[NATIVE_HOST] = dict(counts)
    if counts := ts_fingerprints((ROOT / TS_HOST).read_text()):
        result[TS_HOST] = dict(counts)
    return result


def manifest(text):
    value = json.loads(text)
    if not isinstance(value, dict):
        raise ValueError('manifest must be a path/count map')
    for path, counts in value.items():
        if path not in {NATIVE_HOST, TS_HOST} and not re.fullmatch(r'venues/[A-Za-z0-9_-]+/src/.+\.rs', path):
            raise ValueError('invalid manifest path')
        if not isinstance(counts, dict) or not counts or any(
                not re.fullmatch(r'[a-f0-9]{64}', d) or type(c) is not int or c <= 0
                for d, c in counts.items()):
            raise ValueError('invalid manifest fingerprint/count')
    return value


def check(current, allowed, previous):
    failures = []
    if current != allowed:
        failures.append('current sites must exactly match the inventory (prune removals; no new sites)')
    for label, baseline in previous:
        for path, counts in allowed.items():
            bound = baseline.get(path, {})
            if any(count > bound.get(d, 0) for d, count in counts.items()):
                failures.append(f'{path}: inventory grew against {label}')
    return failures


def git(*args):
    return subprocess.run(['git', '-C', str(ROOT), *args], text=True, capture_output=True, check=True).stdout


def previous_manifests():
    base = os.environ.get('GITHUB_BASE_REF') or 'feat/rust-rewrite'
    relative = git('rev-parse', '--show-prefix').strip() + MANIFEST
    previous = []
    for ref in [f'refs/remotes/origin/{base}']:
        git('rev-parse', '--verify', f'{ref}^{{commit}}')
        if relative in git('ls-tree', '--full-name', '--name-only', ref, '--', MANIFEST).splitlines():
            previous.append((ref, manifest(git('show', f'{ref}:{relative}'))))
    return previous


def main():
    try:
        failures = check(sites(), manifest((ROOT / MANIFEST).read_text()), previous_manifests())
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        failures = [f'cannot verify credential inventory: {type(error).__name__}']
    for failure in failures:
        print('BANNED: credential selection: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    if os.environ.get('CREDENTIAL_INVENTORY_WRITE') == '1':
        (ROOT / MANIFEST).write_text(json.dumps(sites(), indent=2, sort_keys=True) + '\n')
    raise SystemExit(main())
