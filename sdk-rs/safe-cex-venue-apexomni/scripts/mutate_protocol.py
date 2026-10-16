"""One production mutation per protocol row, compiled as one independent corpus.

All row comparisons run even after a mismatch, so one defect cannot mask another.
The Market type/value rows share one production type alias and distinct assertions.
Only two Cargo invocations: mutated corpus, then restored corpus; --jobs 1.
"""
from pathlib import Path
import os
import re
import subprocess

root = Path(__file__).resolve().parents[3]
crate = root / 'sdk-rs/safe-cex-venue-apexomni'
files = [crate / 'src/protocol.rs', crate / 'src/interface.rs']
originals = {p: p.read_text() for p in files}
mutated = dict(originals)
rows = []
for match in re.finditer(r'pub const (\w+)\b[^=]*= (.*?);\n', originals[files[0]], re.S):
    name, body = match.groups()
    changed = body.replace('20_000', '20_001') if name == 'RECV_WINDOW' else re.sub(r'"([^"]*)"', lambda m: '"' + m[1] + '_MUTATED"', body, count=1)
    assert changed != body, name
    mutated[files[0]] = mutated[files[0]].replace(match[0], match[0].replace(body, changed), 1)
    rows.append(name)
for p, native, wire, row in [
    (files[0], 'OrderCancelReason', 'UNKNOWN_ORDER_CANCEL_REASON', 'ORDER_CANCEL_REASON'),
    (files[1], 'ApexMarket', 'BTC-USDT', 'ApexMarket'),
    (files[1], 'OrderSide', 'BUY', 'OrderSide'),
    (files[1], 'OrderType', 'LIMIT', 'OrderType'),
    (files[1], 'TimeInForce', 'GTT', 'TimeInForce'),
    (files[1], 'OrderStatus', 'PENDING', 'OrderStatus'),
]:
    assert f'pub enum {native}' in mutated[p]
    before = mutated[p]
    mutated[p] = before.replace('"' + wire + '"', '"' + wire + '_MUTATED"')
    assert mutated[p] != before, row
    rows.append(row)
mutated[files[1]] = mutated[files[1]].replace('pub type Market = ApexMarket;', 'pub type Market = OrderType;')
rows += ['Market type', 'Market const']
assert len(rows) == 21, rows
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
cmd = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--locked', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-apexomni', '--test', 'protocol', '--jobs', '1', 'matches_live_typescript', '--', '--test-threads=1', '--nocapture']
logs = root / 'sdk-rs/target/apexomni-controls'
logs.mkdir(parents=True, exist_ok=True)
try:
    for p, text in mutated.items():
        p.write_text(text)
    result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / 'protocol-row-mutations.log').write_text(output)
    assert result.returncode == 101 and 'test result: FAILED' in output, output
    for row in rows:
        assert f'ApexOmni protocol row {row}' in output, f'missing named failure {row}: {output}'
    print('Compiled mutation corpus: 21 named row failures at exit 101', flush=True)
finally:
    for p, text in originals.items():
        p.write_text(text)
result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
(logs / 'protocol-row-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('Restored corpus: all 21 row checks passed; 2 Cargo invocations total', flush=True)
