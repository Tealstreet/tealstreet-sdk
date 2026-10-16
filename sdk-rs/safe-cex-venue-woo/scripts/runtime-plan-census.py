"""Independent fixed ten-symbol declaration expectation, plus TOML census."""
from pathlib import Path
from collections import Counter
import json
import subprocess
import tomllib

BASE = 'a07e971bb08f867925ff18ca8b13d2f288d0deba'
ROOT = Path('sdk-rs/parity/manifest/exchanges/woo')
SELECTOR = ROOT / 'woo.v3.runtime-selector.ts.toml'
EXPECTED = {
    'WooV3MigrationRouteId': 'ported',
    'WooV3RuntimeMode': 'ported',
    'WooV3RuntimeRouteTarget': 'divergent',
    'WooV3LegacyRouteSelection': 'divergent',
    'WooV3ShadowReadSelection': 'divergent',
    'WooV3V3RouteSelection': 'divergent',
    'WooV3AlreadyV3RouteSelection': 'divergent',
    'WooV3BlockedRouteSelection': 'ported',
    'WooV3InvalidModeSelection': 'divergent',
    'WooV3RuntimeRouteSelection': 'divergent',
}

def before(path):
    return tomllib.loads(subprocess.check_output(['git', 'show', f'{BASE}:{path}'], text=True))

def census(documents):
    return dict(Counter(r['status'] for d in documents for r in d['symbol']))

old = {r['ts']: r for r in before(SELECTOR)['symbol']}
new = {r['ts']: r for r in tomllib.loads(SELECTOR.read_text())['symbol']}
assert len(new) == 17, 'independent runtime-selector inventory'
changed = {k for k in new if old[k]['status'] != new[k]['status']}
assert changed == set(EXPECTED), (changed, set(EXPECTED))
for symbol, status in EXPECTED.items():
    assert old[symbol]['status'] == 'pending'
    assert new[symbol]['status'] == status
    assert new[symbol]['rs'] == f'safe-cex-venue-woo/src/runtime_selector.rs::{symbol}'
    source = Path('sdk-rs/safe-cex-venue-woo/src/runtime_selector.rs').read_text()
    assert f'pub enum {symbol} ' in source or f'pub struct {symbol} ' in source
    if status == 'divergent':
        assert new[symbol]['note'].startswith(('FORCED:', 'MIXED:'))
paths = sorted(ROOT.glob('*.toml'))
owned = [SELECTOR, ROOT / 'woo.v3.public-parity.ts.toml']
print(json.dumps({
    'base': BASE, 'moved': {'pending_to_ported': 3, 'pending_to_divergent': 7},
    'whole_woo_before': census(before(p) for p in paths),
    'whole_woo_after': census(tomllib.loads(p.read_text()) for p in paths),
    'owned_before': census(before(p) for p in owned),
    'owned_after': census(tomllib.loads(p.read_text()) for p in owned),
    'expectation_floor': 'ten expected symbols; route count 26, route-mode count 544',
}, indent=2))
