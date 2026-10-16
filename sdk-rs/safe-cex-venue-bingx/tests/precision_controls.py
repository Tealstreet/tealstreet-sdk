#!/usr/bin/env python3
"""Compiled mutations and scalar corpus controls, restored after each run."""
import os
from pathlib import Path
import re
import subprocess
crate = Path(__file__).resolve().parents[1]
source, target, oracle = [crate / p for p in ['src/precision.rs', 'tests/precision.rs', 'tests/precision_oracle.cjs']]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-bingx', '--test', 'precision', '--jobs', '1', '--locked']
controls = [
 ('loop-bound', [(source, 'precision_number - 1.0', 'precision_number')], 'native precision'),
 ('decimal-only', [(source, 'parse_int(&text, None)', 'parse_int(&text, Some(10))')], 'native precision'),
]
for operation in ['splice(0)', 'splice(1)', 'push(cases[0])']:
 for bypass in [False, True]:
  edits = [(oracle, 'process.stdout.write', f'cases.{operation};\nprocess.stdout.write')]
  if bypass:
   guard = re.search(r'assert_eq!\(\s*cases.len\(\),\s*45,\s*"BINGX-PARSE source total"\s*\);', target.read_text()).group()
   edits.append((target, guard, ''))
  controls.append((f'{operation}-bypass-{bypass}', edits, 'executed subcorpora' if bypass else 'source total'))
for name, edits, assertion in controls:
 originals = {p:p.read_bytes() for p,_,_ in edits}
 try:
  for p, before, after in edits:
   text=p.read_text(); assert text.count(before)==1
   p.write_text(text.replace(before,after))
  result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101 and f'BINGX-PARSE {assertion}' in result.stdout and 'test result: FAILED' in result.stdout,result.stdout
  print(f'{name}: exit101 BINGX-PARSE {assertion}',flush=True)
 finally:
  for p, text in originals.items():p.write_bytes(text)
 result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 assert result.returncode==0 and '1 passed' in result.stdout,result.stdout
 print('restored 1 passed',flush=True)
