#!/usr/bin/env python3
"""One production defect per new trading/API family; require every named failure."""
import os
import re
import subprocess
from pathlib import Path
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
 ('trading.rs', 'futures_place', 'order_type = "post";', 'order_type = "ioc";', ('futures_place_native',), 'native UTF16'),
 ('trading.rs', 'spot_place', 'h.number(volume)', 'h.number(volume + 1.0)', ('spot_place_native',), 'native UTF16'),
 ('trading.rs', 'futures_update', '("amount", "size", "amount")', '("amount", "size", "price")', ('futures_update_native',), 'native bits'),
 ('trading.rs', 'spot_update', 'vec![field(h, &order, "id")?]', 'vec![id]', ('spot_update_native',), 'native UTF16'),
 ('trading.rs', 'futures_cancel', '("order_id", field(h, &orders[0], "id")?)', '("orderId", field(h, &orders[0], "id")?)', ('futures_cancel_native',), 'native own-key order'),
 ('trading.rs', 'futures_set_leverage', 'let leverage = min(', 'let leverage = 1.0 + min(', ('leverage_native',), 'native bits'),
 ('trading.rs', 'futures_margin', 'text(h, "isolated")', 'text(h, "cross")', ('margin_native',), 'native UTF16'),
 ('api.rs', 'request', 'timestamp * 1000.0', 'timestamp * 1001.0', ('futures_api_native', 'spot_api_native'), 'native UTF16'),
]
originals = {name: (crate / 'src' / name).read_bytes() for name in {c[0] for c in controls}}
mutants = {name: data.decode() for name, data in originals.items()}
for name, function, old, new, tests, assertion in controls:
    source = mutants[name]
    start = re.search(r'\bfn ' + re.escape(function) + r'\b', source).start()
    following = re.search(r'(?m)^\s*(?:pub(?:\([^)]*\))? )?(?:async )?fn \w', source[start + len('fn ' + function):])
    end = start + len('fn ' + function) + following.start() if following else len(source)
    body = source[start:end]
    assert body.count(old) == 1, (function, old, 'unique production anchor')
    mutants[name] = source[:start] + body.replace(old, new) + source[end:]
def run():
    return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-kraken', '--test', 'trading', '--jobs', '2'], env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
try:
    for name, source in mutants.items():
        (crate / 'src' / name).write_text(source)
    broken = run()
    assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout, broken.stdout
    for name, function, old, new, tests, assertion in controls:
        for selected in tests:
            assert f'test {selected} ... FAILED' in broken.stdout, (selected, broken.stdout)
            failure = broken.stdout.split(f'---- {selected} stdout ----', 1)[1].split('----', 1)[0]
            assert assertion in failure, (selected, failure)
            print(f'{function} / {selected}: compiled assertion exit 101; {assertion}', flush=True)
finally:
    for name, original in originals.items():
        (crate / 'src' / name).write_bytes(original)
restored = run()
assert restored.returncode == 0, restored.stdout
for name, function, old, new, tests, assertion in controls:
    for selected in tests:
        assert f'test {selected} ... ok' in restored.stdout, (selected, restored.stdout)
        print(f'{function} / {selected}: restored exit 0', flush=True)
for name, original in originals.items():
    assert (crate / 'src' / name).read_bytes() == original
print(f'{revision}: eight disjoint defects compiled together; nine named failures and nine restored passes; exact source restoration')
