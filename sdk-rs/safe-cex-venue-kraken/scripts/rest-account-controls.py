#!/usr/bin/env python3
"""Discriminate the new REST families against named decoded-native assertions."""
import os
import re
import subprocess
from pathlib import Path
crate = Path(__file__).resolve().parents[1]
production = crate / 'src/rest_account.rs'
original = production.read_bytes()
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
 ('limits', '9007199254740991.0', '9007199254740990.0', ('futures_markets_native', 'spot_markets_native'), 'native bits'),
 ('spot_tickers', '("mark", h.number(last))', '("mark", h.number(last + 1.0))', ('spot_tickers_native',), 'native bits'),
 ('spot_orders', 'max0(volume - filled)', 'max0(volume + filled)', ('spot_orders_native',), 'native bits'),
 ('futures_positions', 'let notional = match old_notional {\n                Some(v) if !h.nullish(&v) => v,', 'let notional = match old_notional {\n                Some(v) if !h.nullish(&v) => h.number(0.0),', ('futures_positions_native',), 'native bits'),
 ('futures_balance', 'let available = max0(available);', 'let available = max0(available) + 1.0;', ('futures_balance_native',), 'native bits'),
 ('spot_refresh_balance', 'h.boolean(true)', 'h.boolean(false)', ('spot_refresh_native',), 'boolean'),
 ('futures_fills', 'oldest + 1.0', 'oldest + 2.0', ('futures_fills_native',), 'UTF16'),
]
def run():
    command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-kraken', '--test', 'rest_account', '--jobs', '2']
    return subprocess.run(command, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
source = original.decode()
# Functions are disjoint; each selected test exercises exactly one mutated family.
for function, old, new, tests, assertion in controls:
    start = source.index('fn ' + function)
    following = re.search(r'(?m)^(?:pub )?(?:async )?fn \w', source[start + len('fn ' + function):])
    end = start + len('fn ' + function) + following.start() if following else len(source)
    body = source[start:end]
    assert body.count(old) == 1, (function, old, 'unique mutation anchor')
    source = source[:start] + body.replace(old, new) + source[end:]
try:
    production.write_text(source)
    broken = run()
    assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout, broken.stdout
    for function, old, new, tests, assertion in controls:
        for selected in tests:
            assert f'test {selected} ... FAILED' in broken.stdout, (selected, broken.stdout)
            failure = broken.stdout.split(f'---- {selected} stdout ----', 1)[1].split('----', 1)[0]
            assert assertion in failure, (selected, failure)
            print(f'{function} / {selected}: compiled native assertion exit 101; {assertion}', flush=True)
finally:
    production.write_bytes(original)
restored = run()
assert restored.returncode == 0, restored.stdout
for function, old, new, tests, assertion in controls:
    for selected in tests:
        assert f'test {selected} ... ok' in restored.stdout, (selected, restored.stdout)
        print(f'{function} / {selected}: restored exit 0', flush=True)
assert production.read_bytes() == original
print(f'{revision}: seven disjoint production defects compiled together; eight named failures, eight restored passes; source restored byte-for-byte')
