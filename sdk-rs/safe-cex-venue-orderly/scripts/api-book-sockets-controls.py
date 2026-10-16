#!/usr/bin/env python3
"""Seven disjoint source-family defects with named native failure/restoration."""
import os, subprocess
from pathlib import Path
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
controls = [
 ('src/api.rs', '"application/json"', '"wrong/json"', 'sign_native', 'native UTF-16 sign/post-query-order'),
 ('src/api.rs', '("retries", h.number(3.0))', '("retries", h.number(4.0))', 'api_native', 'native bits api/default'),
 ('src/public_feeds.rs', 'h.number(1000.0)', 'h.number(1001.0)', 'candles_native', 'native bits candles/connected'),
 ('src/public_feeds.rs', 'get(h, &data, "timestamp")?, false)', 'get(h, &data, "timestamp")?, true)', 'book_native', 'book/buffer-filter-live'),
 ('src/public_feeds.rs', '("amount", amount),\n                ("total", h.number(0.0))', '("amount", amount),\n                ("total", h.number(1.0))', 'process_native', 'native bits process/both-sides'),
 ('src/socket_classes.rs', 'h.define(r.clone(), "topics",', 'h.define(r.clone(), "topicz",', 'public_native', 'native UTF-16 public/shape-pair'),
 ('src/socket_classes.rs', 'name: "ping",', 'name: "pingWrong",', 'private_native', 'native UTF-16 private/shape-pair'),
]
paths = {crate / c[0] for c in controls}
original = {p: p.read_bytes() for p in paths}
sources = {p: b.decode() for p, b in original.items()}
for file, old, new, test, assertion in controls:
 p = crate / file
 assert sources[p].count(old) == (2 if test == 'private_native' else 1), (test, old, sources[p].count(old))
 sources[p] = sources[p].replace(old, new, 1)
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-orderly', '--test', 'api_book_sockets', '--jobs', '2', '--locked'], env=dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'), text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
try:
 for p, s in sources.items(): p.write_text(s)
 broken = run()
 assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout, broken.stdout
 for file, old, new, test, assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout, (test, broken.stdout)
  failure = broken.stdout.split(f'---- {test} stdout ----', 1)[1].split('----', 1)[0]
  assert assertion in failure and 'native' in failure, (test, failure)
  print(f'{test}: compiled {assertion} assertion exit101', flush=True)
finally:
 for p, b in original.items(): p.write_bytes(b)
restored = run()
assert restored.returncode == 0, restored.stdout
for file, old, new, test, assertion in controls:
 assert f'test {test} ... ok' in restored.stdout, (test, restored.stdout)
 print(f'{test}: restored exit0', flush=True)
assert all(p.read_bytes() == b for p, b in original.items())
print(f'{revision}: seven compiled family defects; seven named failures/restored passes; exact bytes restored')
