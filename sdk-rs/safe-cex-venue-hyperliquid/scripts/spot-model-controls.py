#!/usr/bin/env python3
"""Proportionate grouped production controls for the four retained roots.

Run with Python 3.11+, with no other Cargo in flight. Restores production files
in finally blocks. No reference TS file is edited. The report binds to HEAD.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
ROOTS = ['WsFill','WsUserFunding','WsLiquidation','WsNonUserCancel']
assert len(ROOTS) == 4
CARGO = ['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','spot_models','--jobs','2']


def run(command, label, env=None):
    result = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (label + '.log')).write_text(result.stdout)
    print(label + ': exit ' + str(result.returncode), flush=True)
    return result.returncode, result.stdout


def production_control(path, changed, label, names):
    original = path.read_bytes()
    path.write_text(changed)
    try:
        code, output = run(CARGO, label)
    finally:
        path.write_bytes(original)
    restored, restored_output = run(CARGO, label + '-restored')
    record = {'name':label,'exit':code,'restored_exit':restored,'required_named_assertions':names,
              'observed_named_assertions':[name for name in names if name in output],
              'restored_bytes_equal':path.read_bytes()==original}
    report['controls'].append(record)
    # Record surviving assertions before refusing to certify them.
    save()
    assert code == 101 and 'error[E' not in output, (label, code, output)
    assert record['observed_named_assertions'] == names, 'survivor(s): ' + repr(set(names)-set(record['observed_named_assertions']))
    assert restored == 0 and '3 passed; 0 failed' in restored_output, restored_output
    assert record['restored_bytes_equal']


def save():
    args.output.write_text(json.dumps(report, indent=2)+'\n')


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
    logs=Path(tempfile.mkdtemp(prefix='hyperliquid-spot-controls-'))
    report={'measured_sha':sha,'cargo_command':CARGO,'logs':str(logs),'controls':[],
            'scope':'four retained root native assertions; no exhaustive field-pair or historical cardinality claim'}
    save()
    codec=CRATE/'src/spot_json.rs'
    original=codec.read_text()
    anchor='        let units = string_token(raw.get()).map_err(serde::de::Error::custom)?;'
    assert original.count(anchor)==1
    mutation=original.replace(anchor, '        let mut units = string_token(raw.get()).map_err(serde::de::Error::custom)?;\n        if let Some(first) = units.first_mut() { *first ^= 1; }')
    production_control(codec,mutation,'utf16-first-unit-corruption',
        ['SPOTMODEL native '+name+' from_str mode 0' for name in ROOTS])
    # In-memory source mutation through the existing preload; no TS file changes.
    env={**os.environ,'HYPERLIQUID_HERITAGE_SOURCE':'hyperliquidspot.types.ts'}
    for name in ['spot-models-generate','spot-models-oracle']:
        command=['node','--require',str(CRATE/'scripts/heritage-control.cjs'),str(CRATE/'scripts'/(name+'.cjs')),'--check']
        code,output=run(command,name+'-heritage',env)
        report['controls'].append({'name':name+'-heritage','exit':code,'named_failure':'Unsupported interface heritage:','observed':'Unsupported interface heritage:' in output})
        save()
        assert code != 0 and 'Unsupported interface heritage:' in output, output
    report['iteration_stop']='four root corruption assertions, two heritage entry points; all controls restored'
    save()
    print('all four root assertions discriminate; restored gates pass',flush=True)
