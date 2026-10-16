#!/usr/bin/env python3
"""Disjoint compiled defects, one named native failure per new account family."""
import os
import re
import subprocess
from pathlib import Path
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
controls = [
 ('futures_fills','let filled = min(','let filled = 1.0 + min(','fills_native','native bits'),
 ('futures_open_orders','"PostOnly"','"FillOrKill"','open_orders_native','UTF16'),
 ('futures_open_positions','let notional = contracts *','let notional = (contracts + 1.0) *','open_positions_native','native bits'),
 ('futures_balances','free = max0(available);','free = 1.0 + max0(available);','balances_native','native bits'),
 ('spot_parse_order','tif = "PostOnly";','tif = "FillOrKill";','parse_order_native','UTF16'),
 ('spot_executions','&["last_qty", "cum_qty"]','&["cum_qty", "last_qty"]','executions_native','native bits'),
 ('income_range','("pageLimit", h.number(500.0))','("pageLimit", h.number(501.0))','income_range_native','native bits'),
 ('income_history','7.0 * 24.0','8.0 * 24.0','income_history_native','native bits'),
]
path=crate/'src/private_accounts.rs'
original=path.read_bytes()
mutant=original.decode()
for function,old,new,test,assertion in controls:
    match=re.search(r'\bpub (?:async )?fn '+re.escape(function)+r'\b',mutant)
    assert match, function
    start=match.start()
    following=re.search(r'(?m)^\s*(?:pub(?:\([^)]*\))? )?(?:async )?fn \w',mutant[match.end():])
    end=match.end()+following.start() if following else len(mutant)
    body=mutant[start:end]
    assert body.count(old)==1,(function,old,'unique anchor')
    mutant=mutant[:start]+body.replace(old,new)+mutant[end:]
def run():
    return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-kraken','--test','private_accounts','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
    path.write_text(mutant)
    broken=run()
    assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
    for function,old,new,test,assertion in controls:
        assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
        failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
        assert assertion in failure,(test,failure)
        print(f'{function} / {test}: compiled assertion exit 101; {assertion}',flush=True)
finally:
    path.write_bytes(original)
restored=run()
assert restored.returncode==0,restored.stdout
for function,old,new,test,assertion in controls:
    assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
    print(f'{function} / {test}: restored exit 0',flush=True)
assert path.read_bytes()==original
print(f'{revision}: eight compiled defects, eight named native failures, eight restored passes; exact restoration')
