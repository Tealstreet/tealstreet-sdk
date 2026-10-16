#!/usr/bin/env python3
"""Serial scoped verification; six compiling controls, one per new helper."""
import argparse, hashlib, json, os, platform, subprocess, tomllib
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('--gate-sha',required=True)
parser.add_argument('--base-sha',required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parents[3]
os.chdir(root)
crate=Path('sdk-rs/safe-cex-venue-binance')
sha=subprocess.check_output(['git','rev-parse',args.gate_sha],text=True).strip()
base=subprocess.check_output(['git','rev-parse',args.base_sha],text=True).strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha
subprocess.run(['git','merge-base','--is-ancestor',base,sha],check=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
logs=Path('sdk-rs/target/binance-private-helpers')/sha
logs.mkdir(parents=True,exist_ok=True)
inputs=[crate/'src/private_helpers.rs',crate/'src/lib.rs',crate/'tests/private_helpers.rs',crate/'scripts/private-helpers.mjs',Path(__file__).relative_to(root)]
for venue in ['binance','binancepm']:
    inputs += [Path('sdk-rs/parity/manifest/exchanges/binance')/(venue+'.ws-private.ts.toml'),Path('packages/safe-cex/src/exchanges/binance')/(venue+'.ws-private.ts')]
originals={p:p.read_bytes() for p in inputs}
for p,content in originals.items():assert content==subprocess.check_output(['git','show',f'{sha}:{p}']),p
records=[]
def run(label,cmd,exit_code=0,assertion=None):
    p=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (logs/(label+'.log')).write_bytes(p.stdout)
    output=p.stdout.decode(errors='replace')
    records.append(dict(label=label,gate_sha=sha,command=cmd,exit=p.returncode,output=output,log_sha256=hashlib.sha256(p.stdout).hexdigest()))
    print(label,'exit',p.returncode,flush=True)
    assert p.returncode==exit_code,output
    if assertion:
        assert 'Finished `test` profile' in output and assertion in output and 'test result: FAILED. 0 passed; 1 failed;' in output,output
        assert 'left:' in output and 'right:' in output,output
        records[-1]['named_native_failure']=assertion
    elif label in ['baseline','final-restore']:
        assert 'test result: ok. 6 passed; 0 failed; 0 ignored;' in output,output
    elif label=='extractor-check':
        assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in output,output
cargo=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','private_helpers','--jobs','2']
run('baseline',cargo+['--','--include-ignored'])
source=crate/'src/private_helpers.rs'
controls=[]
for label,variant,suffix,test_prefix in [('futures','binance','binance','futures'),('pm','binancepm','binance_pm','pm')]:
    controls += [
        (label+'-wrong-retry-timer','dispose_'+suffix+'_private','clear_timer(state.retry_timer.get())','clear_timer(state.refresh_timer.get())',test_prefix+'_dispose',f'PRIVATE {variant} dispose native cancellation IDs','first cancellation 22 versus 11','normal'),
        (label+'-delay-plus-one','schedule_'+suffix+'_listen_key_refresh','set_timer(delay_ms, callback)','set_timer(delay_ms + 1.0, callback)',test_prefix+'_schedule',f'PRIVATE {variant} schedule native delay','1800001 versus 1800000','normal'),
        (label+'-closing-for-connecting','detach_'+suffix+'_private_socket','ready_state(&previous)? == 0.0','ready_state(&previous)? == 2.0',test_prefix+'_detach',f'PRIVATE {variant} detach native close count','0 versus 1','connecting'),
    ]
for label,function,before,after,test,assertion,pair,case in controls:
    original=originals[source].decode()
    start=original.index('pub fn '+function+'<')
    end=original.find('\n// ts:',start)
    if end<0:end=len(original)
    body=original[start:end]
    assert body.count(before)==1,(label,before)
    try:
        source.write_text(original[:start]+body.replace(before,after)+original[end:])
        run(label,cargo+['--','--include-ignored',test,'--exact'],101,assertion)
        records[-1].update(call_site=str(source)+'::'+function,mutation_before=before,mutation_after=after,native_pair=pair,case_name=case)
    finally:source.write_bytes(originals[source])
    assert source.read_bytes()==originals[source]
run('final-restore',cargo+['--','--include-ignored'])
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'])
run('scoped-rustfmt',['rustfmt','--edition','2024','--check',str(source),str(crate/'tests/private_helpers.rs')])
run('git-diff-check',['git','diff','--check'])
for p,content in originals.items():assert p.read_bytes()==content,p
def census(revision):
    rows={}
    for p in Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.toml'):
        for row in tomllib.loads(subprocess.check_output(['git','show',f'{revision}:{p}'],text=True))['symbol']:
            key=(str(p),row['ts'],row['kind']);assert key not in rows;rows[key]=row['status']
    return rows
before,after=census(base),census(sha)
assert before.keys()==after.keys()
changes=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changes)==6 and all(r['before']=='pending' and r['after']=='ported' for r in changes)
counts=lambda rows:{s:list(rows.values()).count(s) for s in ['ported','divergent','pending']}
evidence=dict(gate_sha=sha,parent_built_on=base,unreviewed_work_in_base=False,moved_pending_to_ported=6,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changes,records=records,restored_inputs_sha256={str(p):hashlib.sha256(v).hexdigest() for p,v in originals.items()},controls_surviving=[],enumeration_floor='Completed 72 authored source cases, two families each: dispose 7 stopping late-refresh; scheduling 13 stopping repeat-callback; detach 16 stopping late-state-change. Stopped after six selected compiling controls, last PM detach connecting case. No exhaustive-input/mutation or whole-venue proof.',scope='Six extracted private-socket helpers with typed data cells and stable u64 configured timer-registry handles, fallible cancel/base/socket/optional-listener operations and numeric ready-state getters. Scheduler starter must resolve current owner method and start its async job when fired. Concrete class/registry/socket/handler/arrow bindings and full asynchronous keepalive ownership remain pending. Source global OPEN=1/CONNECTING=0 are standard configured numeric state constants; custom nonnumeric sockets/mutated globals are outside the boundary.',environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),preliminary_uncommitted_runs=[dict(exit=101,passed=4,failed=2,ignored=0,reason='Source finite-number JSON integer representation compared to serde float representation; changed source comparison to decoded f64 bits, leaving native assertions intact'),dict(exit=0,passed=6,failed=0,ignored=0)],manifest_writer=dict(command=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract'],exit=0,output='Updated 2 manifests; 0 added, 0 orphaned symbols.'),not_run=['full crate test suite','workspace compilation','host clippy','wasm clippy','CI','concrete class/registry/socket/handler/arrow composition','full async keepalive chain','whole-venue audit','cross-engine runtime verification'])
(crate/'PRIVATE_HELPERS_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n')
print('Evidence recorded at',sha,flush=True)
