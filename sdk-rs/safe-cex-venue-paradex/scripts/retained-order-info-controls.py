#!/usr/bin/env python3
"""Exercise each real order-info seam; compiler failures never count as RED."""
from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-paradex'
unit = 'native_exchange::retained_order_info_binding::actual_map_order_receiver_retains_opaque_native_info'
cases = [
    ('member-json-first', 'binding/members.rs', '--lib', unit,
     'let raw = to_retained_input(&x)?;',
     'let raw = safe_cex_types::retained_json::RetainedJsonInput::from(to_json(&x)?);'),
    ('rest-json-first', 'binding/members.rs', '--test', 'retained_info_order_rest_keeps_opaque_native_fields',
     'Ok(Some(to_retained_input(&response)?))',
     'Ok(Some(safe_cex_types::retained_json::RetainedJsonInput::from(to_json(&response)?)))'),
    ('drop-own-undefined', 'order_mapper.rs', '--lib', unit,
     'info: Some(row.clone()).into(),',
     '''info: Some({ let mut raw=row.clone(); if let Some(properties)=raw.get_mut("opaque").and_then(Input::as_object_mut) { properties.retain(|(_,value)| !matches!(value,Input::Undefined)); } raw }).into(),'''),
]
originals = {crate / 'src' / file: (crate / 'src' / file).read_bytes() for _, file, *_ in cases}
env = dict(os.environ, RUSTC_BOOTSTRAP='1')

def run(kind, test, label):
    args = ['cargo','test','--offline','-p','safe-cex-venue-paradex','--jobs','1',kind]
    if kind == '--test': args.append('exchange_trait')
    args += [test,'--','--exact']
    result = subprocess.run(args,cwd=workspace,env=env,capture_output=True,text=True,timeout=900)
    output = result.stdout + result.stderr
    Path('/tmp/paradex-info-control-' + label + '.log').write_text(output)
    return result.returncode,output

try:
    for label,file,kind,test,before,after in cases:
        path = crate / 'src' / file
        original = originals[path]
        assert original.count(before.encode()) == 1,label
        path.write_bytes(original.replace(before.encode(),after.encode()))
        code,output = run(kind,test,label+'-red')
        assert code != 0 and f'test {test} ... FAILED' in output and 'error[E' not in output,output[-5000:]
        print('COMPILED RED: '+label,flush=True)
        path.write_bytes(original)
        code,output = run(kind,test,label+'-green')
        assert code == 0 and '1 passed; 0 failed' in output,output[-5000:]
        print('RESTORED GREEN: '+label,flush=True)
finally:
    for path,source in originals.items(): path.write_bytes(source)
