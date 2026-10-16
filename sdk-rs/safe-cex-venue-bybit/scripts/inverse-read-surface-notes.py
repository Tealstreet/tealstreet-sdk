"""Validate only this read unit's annotation scope; inherited venue findings stay visible."""
from pathlib import Path
import tomllib
root = Path(__file__).resolve().parents[3]
manifest = tomllib.loads((root / 'sdk-rs/parity/manifest/exchanges/bybit/bybitinverse.exchange.ts.toml').read_text())
selected = [row for row in manifest['symbol'] if 'inverse_read_surface.rs::' in str(row['rs'])]
assert len(selected) == 8
for row in selected:
    assert row['status'] == 'ported', row['ts']
    targets = row['rs'] if isinstance(row['rs'], list) else [row['rs']]
    assert targets and all((root / 'sdk-rs' / target.split('::')[0]).is_file() for target in targets)
    assert 'Renamed target ' in row['note'], row['ts']
    if len(targets) > 1:
        assert 'Split:' in row['note'], row['ts']
print('Eight inverse read annotations: targets, rename and split scope; broader Bybit findings excluded.')
