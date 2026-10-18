#!/usr/bin/env python3
"""Independently fault the actual native private methods and restore exact bytes."""
import hashlib
import json
import re
import subprocess
from pathlib import Path
crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / 'src/private_classes.rs'
original = source.read_bytes()
logs = workspace / 'target/private-native-owner-controls'
logs.mkdir(parents=True, exist_ok=True)
receipt = {'sourceSha256': hashlib.sha256(original).hexdigest(), 'controls': []}
command = ['node', str(crate / 'scripts/private-native-owner-contracts.mjs'), '--native']
def run(label, owner, mismatch=False):
    file = logs / (label + '.log')
    with file.open('w') as stream:
        status = subprocess.run(command + ['--only=' + owner] + (['--expect-mismatch'] if mismatch else []),
                                cwd=workspace, stdout=stream, stderr=subprocess.STDOUT).returncode
    output = file.read_text()
    assert status == 0, output[-6000:]
    assert not re.search(r'error\[E\d+\]|could not compile|runtime probes did not execute', output), output[-6000:]
    rows = json.loads((workspace / 'target/accepted-inventory/accepted-set-results.json').read_text())
    assert len(rows) == 1 and rows[0]['name'].endswith('::' + owner)
    assert 'native' in rows[0]['compilerFeatures'] and not rows[0]['classificationOnly']
    if mismatch:
        assert any('function output mismatch' in f for f in rows[0]['failures'])
    else:
        assert rows[0]['failures'] == []
    return rows[0]
controls = [
 ('hmac', 'Auth', 'crate::api_factory::utf8_hmac_sha256_hex(secret, payload)',
  'crate::api_factory::utf8_hmac_sha256_hex(secret, format!("{payload}-fault"))'),
 ('event-routing', 'PrivateState',
  'pub fn handle_events(self: &Rc<Self>, kind: u8, events: EventValue) -> Result<(), Failure> {',
  'pub fn handle_events(self: &Rc<Self>, kind: u8, events: EventValue) -> Result<(), Failure> {\n        if kind == 1 { return Ok(()); }'),
 ('emit', 'PrivateState', 'self.base().emit_error(msg)',
  'self.base().emit_error(format!("{msg}-fault"))'),
 ('log', 'PrivateState', 'self.parent.log(msg, severity)',
  'self.parent.log(format!("{msg}-fault"), severity)'),
 ('create', 'PrivateState', 'let socket = self.base().create(url)?;',
  'let socket = self.base().create(format!("{url}/fault"))?;'),
 ('listener', 'PrivateState',
  "fn add_listener(&self, _ws: EventValue, event: &'static str) -> Result<(), Failure> {",
  "fn add_listener(&self, _ws: EventValue, event: &'static str) -> Result<(), Failure> {\n        if event == \"message\" { return Ok(()); }"),
]
baseline = logs / 'all-baseline.log'
with baseline.open('w') as stream:
    status = subprocess.run(command, cwd=workspace, stdout=stream, stderr=subprocess.STDOUT).returncode
assert status == 0, baseline.read_text()[-6000:]
initial = json.loads((workspace / 'target/accepted-inventory/accepted-set-results.json').read_text())
assert {r['name'].rsplit('::', 1)[-1] for r in initial} == {'Auth', 'PrivateState'}
assert len(initial) == 2 and all(not r['classificationOnly'] and not r['failures']
                               and 'native' in r['compilerFeatures'] for r in initial)
for label, owner, before, after in controls:
    if (logs / 'STOP').exists():
        print('Stopped between controls with production source restored.', flush=True)
        break
    text = original.decode()
    assert text.count(before) == 1, label
    try:
        source.write_text(text.replace(before, after))
        red = run(label + '-compiled-red', owner, True)
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    run(label + '-restored-green', owner)
    receipt['controls'].append({'role': label, 'owner': red['name'],
                                'compiledNamedMismatch': True, 'exactRestorationGreen': True})
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(label + ': compiled native owner mismatch RED; exact restoration GREEN', flush=True)
