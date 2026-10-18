"""Require live socket effects and source unit observations to fail before wire."""
from pathlib import Path
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode == 0
out = root/'sdk-rs/target/apexomni-socket-frame-controls'/sha
out.mkdir(parents=True,exist_ok=True)
receipt = {'sha':sha,'attempts':[],'survivors':[],'invalid_controls':[]}


def run(name, expected, marker=None, extra=None):
    command=[os.environ.get('CARGO','cargo'),'test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),
             '--locked','--jobs','1','-p','safe-cex-venue-apexomni','--test','exchange',
             'websocket_tests::socket_frame_units_are_observed_before_wire','--','--exact','--nocapture']
    start=time.monotonic()
    result=subprocess.run(command,cwd=root/'sdk-rs',env={**os.environ,**(extra or {})},capture_output=True,text=True)
    text=result.stdout+result.stderr
    (out/(name+'.log')).write_text(text)
    valid=result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name':name,'exit':result.returncode,'expected_exit':expected,
                               'assertion':marker,'valid':valid,'seconds':round(time.monotonic()-start,2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control':name,'call_site':'private/public on_message'})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out/'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(name,result.returncode,'valid',valid,flush=True)
    return valid


cases=[
    ('private-ping-gate','ws_private.rs','if get(&json, "op") == &retained_text("ping")',
     'if get(&json, "op") == &retained_text("ignored")',
     'APEX-WS-FRAME private/high native transmission count'),
    ('public-handler-gate','ws_public.rs','handler(&json)?;',
     'if topic != "instrumentInfo.all" { handler(&json)?; }',
     'APEX-WS-FRAME public/pair native ticker last'),
]
assert len(cases)==2
for name,filename,before,after,marker in cases:
    path=root/'sdk-rs/safe-cex-venue-apexomni/src'/filename
    original=path.read_bytes();source=original.decode()
    start=source.index('    pub fn on_message(')
    end=source.index('    pub fn ',start+10)
    body=source[start:end]
    assert body.count(before)==1,(name,body.count(before))
    try:
        path.write_text(source[:start]+body.replace(before,after)+source[end:])
        run(name+'-mutation',101,marker)
    finally:
        path.write_bytes(original)
    assert run(name+'-restored',0),name
for kind,marker in [('private','APEX-WS-FRAME private/high source peer argument units'),
                    ('public','APEX-WS-FRAME public/high source updated ticker')]:
    run(kind+'-source-unit-substitution',101,marker,{'SAFE_CEX_APEX_SOCKET_FRAME_CONTROL':kind})
    assert run(kind+'-source-unit-restored',0),kind
assert len(receipt['attempts'])==8,receipt
assert not receipt['survivors']and not receipt['invalid_controls'],receipt
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode == 0
