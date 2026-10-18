#!/usr/bin/env python3
"""Independently corrupt actual member and request future forwarding."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
sources = {name: crate / ('src/' + name + '.rs') for name in ['private_rest', 'native_host']}
originals = {name: file.read_bytes() for name, file in sources.items()}
logs = workspace / 'target/snapshot-future-controls'
logs.mkdir(parents=True, exist_ok=True)
name = 'snapshot_future_binding::snapshot_futures_wait_sequentially_and_retain_native_results'
command = ['cargo', 'test', '--color', 'never', '--locked', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--jobs', '1', '--test', 'private_classes', name, '--', '--exact']
receipt = {'sourceSha256': {key: hashlib.sha256(value).hexdigest() for key,value in originals.items()}, 'controls': []}


def execute(label):
    file = logs / (label + '.log')
    with file.open('w') as output:
        result = subprocess.run(command, cwd=workspace, env={**os.environ, 'CARGO_BUILD_JOBS':'1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, file.read_text()


for alias, source, before, after in [
    ('SnapshotFuture', 'private_rest', 'let first = regular()?.await?; // source spreads first result before calling second method',
     'let first = conditional()?.await?; let _ = &mut regular; // faulty member order'),
    ('SnapshotRequestFuture', 'native_host', 'result: request.await,',
     'result: request.await.map(|_| EventValue::Undefined),'),
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
        receipt['controls'].append({'owner':'safe_cex_venue_binance::private_rest::'+alias,'test':name,'namedRuntimeFailure':True})
    finally:
        for key,file in sources.items():
            file.write_bytes(originals[key])
            assert file.read_bytes() == originals[key]
    status,log = execute(alias + '-exact-restoration-green')
    assert status == 0 and '1 passed; 0 failed' in log,log[-3000:]
    (logs / 'results.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(alias+': compiled native RED; exact restoration GREEN',flush=True)
print('Both actual snapshot continuations have independent compiled controls.',flush=True)
