#!/usr/bin/env python3
"""Nine disjoint read-family defects, named native assertions and restoration."""
import os, subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/collection_reads.rs';saved=p.read_bytes();source=saved.decode()
controls=[
 ('let p = call(h, r.clone(), "fetchBalanceAndPositionsRaw", vec![])?;', 'let p = call(h, r.clone(), "fetchOpenPositions", vec![])?;', 'open_native','open/delegate'),
 ('h.set(&obj, k, h.number(0.0))?;', 'h.set(&obj, k, h.number(1.0))?;', 'positions_native','positions/missing-metadata'),
 ('("balance", balance)', '("balanceWrong", balance)', 'balance_native','balance/identity'),
 ('h.flatten(orders)', 'Ok(orders)', 'orders_native','orders/serial-flat'),
 ('h.sleep(100.0)?', 'h.sleep(101.0)?', 'emit_native','emit/priority-dedupe'),
 ('h.boolean(h.equal(&get(h, &order, "action")?, &h.text("CLOSE")))', 'h.boolean(!h.equal(&get(h, &order, "action")?, &h.text("CLOSE")))', 'symbol_native','symbol/mapped-orders'),
 ('h.arithmetic(Op::Min, v, h.number(1000.0))', 'h.arithmetic(Op::Min, v, h.number(999.0))', 'rows_native','rows/clamped'),
 ('h.arithmetic(Op::Mul, numeric, h.number(1000.0))?', 'h.arithmetic(Op::Mul, numeric, h.number(1001.0))?', 'history_native','history/sort-and-dedupe-id'),
 ('Op::Div, get(h, &t, "time")?, h.number(1000.0)', 'Op::Div, get(h, &t, "time")?, h.number(1.0)', 'fills_native','fills/carry-fields'),
]
for old,new,test,assertion in controls:
 assert source.count(old)==(2 if test=='open_native' else 1),(test,source.count(old),old)
 source=source.replace(old,new,1)
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','collection_reads','--jobs','2','--locked'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for old,new,test,assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
  assert 'native' in failure and assertion in failure,(test,failure)
  print(f'{test}: compiled native {assertion} assertion exit101',flush=True)
finally:p.write_bytes(saved)
restored=run();assert restored.returncode==0,restored.stdout
for old,new,test,assertion in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit0',flush=True)
assert p.read_bytes()==saved
print(f'{revision}: nine compiled defects; nine named native failures/restored passes; exact bytes restored')
