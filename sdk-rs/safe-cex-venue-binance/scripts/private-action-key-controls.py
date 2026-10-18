#!/usr/bin/env python3
"""Independently corrupt live captured Action and native PinKey forwarding."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
sources = {name: crate / ('src/' + name + '.rs') for name in ['private_classes', 'exchange_classes']}
originals = {name: file.read_bytes() for name, file in sources.items()}
logs = workspace / 'target/private-action-key-controls'
logs.mkdir(parents=True, exist_ok=True)
name = ''
command = ['cargo', 'test', '--color', 'never', '--locked', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--jobs', '1', '--test', 'private_classes']
receipt = {'sourceSha256': {key: hashlib.sha256(value).hexdigest() for key,value in originals.items()}, 'controls': []}


def execute(label):
    file = logs / (label + '.log')
    with file.open('w') as output:
        result = subprocess.run(command + [name, '--', '--exact'], cwd=workspace, env={**os.environ, 'CARGO_BUILD_JOBS':'1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, file.read_text()


for alias, source, before, after, name in [
    ('Action', 'private_classes', '(self.function)()', 'Ok(())',
     'production_native_private_clones_share_settlement_and_captured_receiver'),
    ('PinKey', 'exchange_classes', 'let value = data.read("listenKey")?;',
     'let value = EventValue::Undefined; let _ = data;',
     'private_action_key_binding::private_pin_key_retains_native_value_and_request_rejection_after_suspension'),
]:
    status, log = execute(alias + '-baseline-green')
    assert status == 0 and '1 passed; 0 failed' in log, log[-3000:]
    text = originals[source].decode()
    assert text.count(before) == 1, alias
    try:
        sources[source].write_text(text.replace(before,after))
        status, log = execute(alias + '-red')
        assert status != 0, alias + ': corrupted continuation passed'
        assert not re.search(r'^error(?:\[|: (?!test failed\b))',log,re.M), alias + ': compiler failure does not count'
        assert f'test {name} ... FAILED' in log, alias + ': named native runtime failure required'
        assert '0 passed; 1 failed' in log
        receipt['controls'].append({'owner':'safe_cex_venue_binance::private_classes::'+alias,'test':name,'namedRuntimeFailure':True})
    finally:
        for key,file in sources.items():
            file.write_bytes(originals[key])
            assert file.read_bytes() == originals[key]
    status,log = execute(alias + '-exact-restoration-green')
    assert status == 0 and '1 passed; 0 failed' in log,log[-3000:]
    (logs / 'results.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(alias+': compiled native RED; exact restoration GREEN',flush=True)
print('Both private Action/PinKey aliases have independent compiled controls.',flush=True)
