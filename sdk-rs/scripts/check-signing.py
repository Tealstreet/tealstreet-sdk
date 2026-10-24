#!/usr/bin/env python3
"""Freeze legacy credential/signing sites while families migrate to dispatch.

The hash is of one stripped source line (not its line number), so ordinary
refactors cannot silently introduce another secret holder. Legacy sign modules are reviewed during their migration. Migrated Binance, Bybit, BingX, AsterDEX, OKX, Bitget, Phemex and KuCoin
include their sign modules in this ban: only frozen test-oracle sites remain;
production primitive work uses the provider. Test-only sites are frozen too.
"""
import collections
import hashlib
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
PATTERN = re.compile(r'\b(?:hmac\w*|ed25519\w*|\w*secret\w*)\b', re.I)


def sites():
    found = collections.defaultdict(collections.Counter)
    for path in sorted((ROOT / 'venues').glob('*/src/**/*.rs')):
        relative = path.relative_to(ROOT / 'venues')
        if relative.parts[0] not in {'asterdex', 'binance', 'bingx', 'bybit', 'okx', 'bitget', 'phemex', 'kucoin'} and (path.name == 'sign.rs' or 'sign' in relative.parts[2:-1]):
            continue
        for line in path.read_text().splitlines():
            if not line.strip().startswith('//') and PATTERN.search(line):
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
