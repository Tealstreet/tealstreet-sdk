#!/usr/bin/env python3
"""One production defect per book/listener family, with separate venue selections."""
from pathlib import Path
import os
import re
import subprocess
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
 ('orderbooks.rs', 'captured_levels', 'quantity <= 0.0', 'quantity <= 20.0', ('futures_snapshot_live_source', 'spot_snapshot_live_source'), 'BOOK native event'),
 ('orderbooks.rs', 'futures_delta', 'quantity == 0.0', 'quantity == 1.0', ('futures_delta_live_source',), 'BOOK native event'),
 ('orderbooks.rs', 'captured_levels', 'quantity == 0.0', 'quantity == 1.0', ('spot_delta_live_source',), 'BOOK native event'),
 ('orderbooks.rs', 'emitted_levels', '.take(100)', '.take(99)', ('futures_emit_live_source', 'spot_emit_live_source'), 'BOOK native array length'),
 ('book_listeners.rs', 'futures_listen_book', 'listener.clone(), 100', 'listener.clone(), 101', ('futures_listener_live_source',), 'LISTEN native number'),
 ('book_listeners.rs', 'spot_listen_book', 'listener.clone(), 100', 'listener.clone(), 101', ('spot_listener_live_source',), 'LISTEN native number'),
 ('book_listeners.rs', 'spot_frame', 'channel: "ohlc"', 'channel: "book"', ('ohlcv_listener_live_source',), 'LISTEN native UTF16'),
]
originals = {name: (crate / 'src' / name).read_bytes() for name in {c[0] for c in controls}}
for name, function, old, new, tests, assertion in controls:
 production = crate / 'src' / name
 original = originals[name]
 source = original.decode()
 start = source.index('fn ' + function)
 following = re.search(r'(?m)^\s*(?:pub )?(?:async )?fn \w', source[start + len('fn ' + function):])
 end = start + len('fn ' + function) + following.start() if following else len(source)
 body = source[start:end]
 assert body.count(old) == 1, (function, 'unique mutation anchor', old)
 def run(selected):
  command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-kraken', '--test', 'orderbooks', selected, '--jobs', '2']
  return subprocess.run(command, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
 for selected in tests:
  baseline = run(selected)
  assert baseline.returncode == 0, baseline.stdout
 try:
  production.write_text(source[:start] + body.replace(old, new) + source[end:])
  for selected in tests:
   broken = run(selected)
   assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout, (function, selected, broken.stdout)
   print(f'{function} / {selected}: compiled exit 101; {assertion}', flush=True)
 finally:
  if production.read_bytes() != original:
   production.write_bytes(original)
 for selected in tests:
  restored = run(selected)
  assert restored.returncode == 0, restored.stdout
  assert production.read_bytes() == original
  print(f'{function} / {selected}: restored exit 0', flush=True)
print(f'{revision}: 9 rows; 7 defects, 9 selected named failures and 9 restored passes')
