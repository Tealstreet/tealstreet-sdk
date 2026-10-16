#!/usr/bin/env python3
"""Nine disjoint data defects, named failures and exact source restoration."""
import os, subprocess
from pathlib import Path
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
p = crate / 'src/data_mappers.rs'
original = p.read_bytes()
source = original.decode()
controls = [
 ('pub async fn markets', '("active", h.boolean(true))', '("active", h.boolean(false))', 'markets_native', 'native boolean'),
 ('pub async fn tickers', '("bid", close.clone())', '("bid", text(h, "WRONG"))', 'tickers_native', 'native UTF-16'),
 ('pub async fn positions', 'h.arithmetic(Op::Sub, total.clone(), free)?', 'h.arithmetic(Op::Div, total.clone(), free)?', 'positions_native', 'native bits'),
 ('pub fn map_position', 'h.number(holdings.abs())', 'h.number(holdings.abs() + 1.0)', 'position_mapper_native', 'native bits'),
 ('pub fn map_limit', '("info", o)', '("info", h.undefined())', 'limit_mapper_native', 'native original output handle'),
 ('pub fn map_algo', '("price", price)', '("price", h.arithmetic(Op::Mul, price, h.number(0.0))?)', 'algo_mapper_native', 'native bits'),
 ('pub async fn candles', 'h.number(1000.0)', 'h.number(1.0)', 'candles_native', 'native bits'),
 ('pub fn listen_candles', 'vec![options, callback]', 'vec![options, { drop(callback); h.undefined() }]', 'listen_candles_native', 'native boolean'),
 ('pub fn listen_book', 'vec![symbol, callback]', 'vec![symbol, { drop(callback); h.undefined() }]', 'listen_book_native', 'native boolean'),
]
for start, old, new, test, assertion in controls:
 a = source.index(start)
 b = source.find('\npub ', a + len(start))
 b = len(source) if b == -1 else b
 region = source[a:b]
 expected = 2 if start == 'pub async fn candles' else 1
 assert region.count(old) == expected, (start, old, region.count(old))
 # Candle defect changes timestamp divisor, leaving request cap intact.
 region = region.replace(old, new) if expected == 1 else region.rsplit(old, 1)[0] + new + region.rsplit(old, 1)[1]
 source = source[:a] + region + source[b:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-orderly', '--test', 'data_mappers', '--jobs', '2'], env=dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'), text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
try:
 p.write_text(source)
 broken = run()
 assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout, broken.stdout
 for start, old, new, test, assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout, (test, broken.stdout)
  failure = broken.stdout.split(f'---- {test} stdout ----', 1)[1].split('----', 1)[0]
  assert assertion in failure, (test, failure)
  print(f'{test}: compiled native assertion exit 101 ({assertion})', flush=True)
finally:
 p.write_bytes(original)
restored = run()
assert restored.returncode == 0, restored.stdout
for start, old, new, test, assertion in controls:
 assert f'test {test} ... ok' in restored.stdout, (test, restored.stdout)
 print(f'{test}: restored exit 0', flush=True)
assert p.read_bytes() == original
print(f'{revision}: nine compiled disjoint defects; nine named failures/restored passes; exact bytes restored')
