"""Discriminate the actual source capture point and native poll-time Store lookup."""
from pathlib import Path
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode == 0
out = root/'sdk-rs/target/apexomni-update-calltime-controls'/sha
out.mkdir(parents=True,exist_ok=True)
receipt = {'sha':sha,'attempts':[],'survivors':[],'invalid_controls':[]}


def run(name, expected, marker=None, extra=None):
    command = [os.environ.get('CARGO','cargo'),'test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),
               '--locked','--jobs','2','-p','safe-cex-venue-apexomni','--test','exchange',
               'trading_tests::update_order_store_selection_begins_at_first_poll','--','--exact','--nocapture']
    start = time.monotonic()
    result = subprocess.run(command,cwd=root,env={**os.environ,**(extra or {})},capture_output=True,text=True)
    text = result.stdout+result.stderr
    (out/(name+'.log')).write_text(text)
    valid = result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name':name,'exit':result.returncode,'expected_exit':expected,
                               'assertion':marker,'valid':valid,'seconds':round(time.monotonic()-start,2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control':name,'call_site':'ApexOmniExchange::update_order'})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out/'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(name,result.returncode,'valid',valid,flush=True)
    return valid


path = root/'sdk-rs/safe-cex-venue-apexomni/src/trading.rs'
original = path.read_bytes()
source = original.decode()
before = '== Some(base_id)'
assert source.count(before) == 1
try:
    path.write_text(source.replace(before,'== Some("missing")'))
    run('native-lookup-mutation',101,'APEX-UPDATE native first poll sees filled Store')
finally:
    path.write_bytes(original)
assert run('native-lookup-restored',0)
run('source-deferred-prefix-mutation',101,'APEX-UPDATE clear-after-call source eager selection succeeds',
    {'SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL':'update'})
assert run('source-deferred-prefix-restored',0)
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode == 0
assert not receipt['survivors'] and not receipt['invalid_controls'],receipt
