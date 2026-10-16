#!/usr/bin/env python3
"""One reversible production control covering the fourteen new spot roots.

Mutate one key/value in each spot table and swap the shared TIF, TP/SL and
Grouping decoded labels. The three enum swaps must fail all six spot type
consumers, including the three composite records. Every named native assertion
is required; an accepted-set/compile failure alone is insufficient.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

crate = Path(__file__).resolve().parents[1]
root = crate.parents[1]
spot = crate / 'src/spot_types.rs'
core = crate / 'src/types.rs'
originals = {path: path.read_text() for path in (spot, core)}
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
logs = Path(tempfile.mkdtemp(prefix='hyperliquid-spot-control-'))
command = ['cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p',
           'safe-cex-venue-hyperliquid', '--test', 'spot_types', '--jobs', '2', '--', '--nocapture']
environment = dict(os.environ)
environment.setdefault('SAFE_CEX_TYPESCRIPT_PATH', '/Users/sam/cs/tealstreet-next/node_modules/typescript')
names = ['BASE_URLS', 'WSS_URLS', 'ENDPOINTS', 'GRAFANA_ENDPOINTS_EVERY',
         'GRAFANA_ENDPOINTS_PERIODIC', 'INFO_TYPES', 'EXCHANGE_TYPES', 'WEBSOCKET']
required = ['SPOT native table ' + name for name in names] + [
    'SPOT native Tif variant case 0', 'SPOT native Tpsl variant case 7',
    'SPOT native Grouping variant case 13', 'SPOT native LimitOrderType TIF case 20',
    'SPOT native TriggerOrderType TP/SL case 27', 'SPOT native OrderType limit TIF case 37',
]
try:
    text = originals[spot]
    for name in names:
        pattern = r'(pub const ' + name + r'\b[^;]+;)'
        def mutate(match):
            block, count = re.subn(r'"([^"\n]+)"', lambda item: '"' + item[1] + '#CONTROL"', match[1], count=1)
            assert count == 1, name
            return block
        text, count = re.subn(pattern, mutate, text)
        assert count == 1, name
    spot.write_text(text)
    text = originals[core]
    for enum, left, right in [('TifWire', 'Alo', 'Ioc'), ('TpslWire', 'tp', 'sl'), ('GroupingWire', 'na', 'normalTpsl')]:
        pattern = r'(enum ' + enum + r'\s*\{[^}]+\})'
        def swap(match):
            block = match[1]
            a, b = f'rename = "{left}"', f'rename = "{right}"'
            assert block.count(a) == block.count(b) == 1, enum
            return block.replace(a, 'rename = "#TEMP"').replace(b, a).replace('rename = "#TEMP"', b)
        text, count = re.subn(pattern, swap, text)
        assert count == 1, enum
    core.write_text(text)
    mutant = subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (logs / 'mutant.log').write_text(mutant.stdout)
finally:
    for path, content in originals.items():
        path.write_text(content)
restored = subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
(logs / 'restored.log').write_text(restored.stdout)
print(json.dumps({'gate_sha':sha,'cargo_mutant_exit':mutant.returncode,'cargo_restored_exit':restored.returncode,
                  'named_native_failures':[name for name in required if name in mutant.stdout],
                  'restored_files_equal':all(path.read_text() == text for path,text in originals.items()),'logs':str(logs)},indent=2))
assert mutant.returncode == 101, 'mutant must reach failing Rust test assertions'
assert '7 failed' in mutant.stdout, 'all seven native tests must fail'
assert all(name in mutant.stdout for name in required), 'missing named native assertion; survivor is a finding'
assert restored.returncode == 0 and '7 passed' in restored.stdout, 'restored tests must pass'
