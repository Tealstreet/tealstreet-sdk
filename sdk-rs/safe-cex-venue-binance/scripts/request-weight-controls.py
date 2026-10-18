#!/usr/bin/env python3
"""Require compiled production failures and exact restoration for weight helpers."""
import json
import os
from pathlib import Path
import subprocess
import sys
if not __debug__:
    raise SystemExit('Controls require Python assertions; do not use -O')
workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-binance/src/request_weight.rs'
original = source.read_bytes()
logs = workspace / 'target/request-weight-controls'
logs.mkdir(parents=True, exist_ok=True)
binding = 'request_weight_binding::native_response_weight_pauses_actual_owner_at_threshold'
command = ['cargo','test','--locked','--jobs','2','-p','safe-cex-venue-binance','--features','native','--test','private_classes',binding,'--','--exact']

def run(label):
    result = subprocess.run(command,cwd=workspace,capture_output=True,text=True,timeout=1200)
    text = result.stdout + result.stderr
    (logs/(label+'.log')).write_text(text)
    return result.returncode,text

controls = [
 ('header', b'if !has_headers {', b'if has_headers {'),
 ('weight', b'used >= 1_920.0', b'used >= 9_999.0'),
]
assert len(sys.argv) == 1
try:
    for name,before,after in controls:
        assert source.read_bytes() == original
        assert original.count(before) == 1
        source.write_bytes(original.replace(before,after))
        code,text = run(name+'-red')
        assert code == 101 and f'test {binding} ... FAILED' in text and 'could not compile' not in text,text[-8000:]
        print(name+': compiled named real-native RED',flush=True)
        source.write_bytes(original)
        assert source.read_bytes() == original
        code,text = run(name+'-restored')
        assert code == 0 and '1 passed; 0 failed' in text and f'test {binding} ... ok' in text,text[-8000:]
        print(name+': byte-exact restored native GREEN',flush=True)
    # A scalar error constructor cannot be reached by decoded string-only HTTP
    # headers. Execute its actual private production binding through the driver.
    before,after = b'error.name = "TypeError".into();', b'error.name = "Error".into();'
    assert original.count(before) == 1
    target = workspace/'target/accepted-inventory'
    env = dict(os.environ,RUSTC_BOOTSTRAP='1',CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(target))
    measure = ['node','safe-cex-parity/scripts/accepted-set.mjs','--package=safe-cex-venue-binance','--jobs=2','--measure','--owner=request_weight::type_error']
    for label,body,failing in [('type-error-red',original.replace(before,after),True),('type-error-restored',original,False)]:
        source.write_bytes(body)
        result=subprocess.run(measure,cwd=workspace,env=env,capture_output=True,text=True,timeout=1200)
        (logs/(label+'.log')).write_text(result.stdout+result.stderr)
        assert result.returncode == 0,(result.stdout+result.stderr)[-8000:]
        rows=json.loads((target/'accepted-set-results.json').read_text())
        assert len(rows)==1 and rows[0]['name']=='safe_cex_venue_binance::request_weight::type_error'
        assert not rows[0].get('classificationOnly')
        assert bool(rows[0]['failures']) == failing,rows
        if failing: assert all('compile failure' not in s for s in rows[0]['failures']),rows
        (logs/(label+'.json')).write_text(json.dumps(rows,indent=2)+'\n')
        print(label+': executed private production output '+('RED' if failing else 'GREEN'),flush=True)
finally:
    source.write_bytes(original)
