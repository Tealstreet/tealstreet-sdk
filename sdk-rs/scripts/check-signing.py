#!/usr/bin/env python3
"""Freeze legacy credential/signing sites while families migrate to dispatch.

The hash is of one stripped source line (not its line number), so ordinary
refactors cannot silently introduce another secret holder. Legacy sign modules are reviewed during their migration. Migrated Binance, Bybit, BingX, AsterDEX, OKX, Bitget and Phemex
include their sign modules in this ban: only frozen test-oracle sites remain;
production primitive work uses the provider. Test-only sites are frozen too.
"""
import collections
import hashlib
import importlib.util
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
# Engine and workspace-macro sources the crate-wide proof reads; never a fixture root.
SDK = pathlib.Path(__file__).resolve().parents[1]
PATTERN = re.compile(r'\b(?:hmac\w*|ed25519\w*|\w*secret\w*)\b', re.I)
# A dispatch recipe names its provider algorithm (`Algorithm::HmacSha256`); that
# declaration holds no material. Only a proven engine enum variant is ignored;
# calls, other paths and local or aliased `Algorithm`s, and the rest of the line,
# still count.
VARIANTS = {'HmacSha256', 'HmacSha512', 'Sha256SecretSuffix', 'Ed25519', 'Es256',
            'Secp256k1Digest', 'StarkDigest', 'SchnorrDigest', 'ZkLinkContract'}
ENGINE_ALGORITHM = ['safe_cex_engine', 'signing', 'Algorithm']


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, pathlib.Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_tokens = _load('request_tokens', 'check-request-context.py').tokens
rust_scope = _load('rust_scope', 'rust_scope.py')


def declared_variants(source, crate_proven=True):
    """Line number -> count of proven engine `Algorithm::<variant>` uses; none unless
    the crate-wide proof closes every generator and glob and leaves the engine root
    unshadowed."""
    if not crate_proven:
        return collections.Counter()
    try:
        positioned = _tokens(source, positions=True)
    except ValueError:
        return collections.Counter()
    stream = [text for text, _ in positioned]
    found = collections.Counter()
    for i in range(len(stream) - 2):
        if stream[i] != 'Algorithm' or stream[i + 1] != '::' or stream[i + 2] not in VARIANTS:
            continue
        if stream[i + 3:i + 4] in (['('], ['::'], ['!'], ['{']):
            continue
        qualified = stream[i - 4:i] == ['safe_cex_engine', '::', 'signing', '::']
        if stream[i - 1:i] == ['::'] and not qualified:
            continue
        try:
            proven = qualified or rust_scope.resolve(stream, 'Algorithm', i, ENGINE_ALGORITHM) == 'provider'
        except ValueError:
            proven = False
        if proven:
            found[source.count('\n', 0, positioned[i][1]) + 1] += 1
    return found


def counted(line, declared):
    """Whether a line holds a site after removing its proven declarations."""
    if declared and len(re.findall(r'\bAlgorithm::\w+', line)) == declared:
        line = re.sub(r'\bAlgorithm::\w+', '', line)
    return bool(PATTERN.search(line))


def proven_crates():
    """Venue crates whose sources pass the crate-wide proof for the engine root."""
    by_crate = collections.defaultdict(dict)
    for path in sorted((ROOT / 'venues').glob('*/src/**/*.rs')):
        by_crate[path.relative_to(ROOT / 'venues').parts[0]][path] = _tokens(path.read_text())
    return {crate for crate, streams in by_crate.items()
            if not rust_scope.crate_refusals(SDK, streams, {'safe_cex_engine'})}


def sites():
    found = collections.defaultdict(collections.Counter)
    proven = proven_crates()
    for path in sorted((ROOT / 'venues').glob('*/src/**/*.rs')):
        relative = path.relative_to(ROOT / 'venues')
        if relative.parts[0] not in {'asterdex', 'binance', 'bingx', 'bybit', 'okx', 'bitget', 'phemex', 'bitrue'} and (path.name == 'sign.rs' or 'sign' in relative.parts[2:-1]):
            continue
        source = path.read_text()
        declared = declared_variants(source, relative.parts[0] in proven)
        for number, line in enumerate(source.splitlines(), 1):
            if not line.strip().startswith('//') and counted(line, declared[number]):
                digest = hashlib.sha256(line.strip().encode()).hexdigest()
                found[str(path.relative_to(ROOT))][digest] += 1
    return found


def main():
    baseline = json.loads((ROOT / 'scripts/signing-legacy-sites.json').read_text())
    failures = []
    for path, counts in sites().items():
        for digest, count in counts.items():
            if count > baseline.get(path, {}).get(digest, 0):
                failures.append(f'{path}: new credential/crypto site ({digest[:12]})')
    for failure in failures:
        print('BANNED: signing outside the dispatch sign step: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
