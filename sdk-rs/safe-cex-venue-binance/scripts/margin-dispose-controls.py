#!/usr/bin/env python3
"""Serial scoped gates with four compiling controls, one per newly ported row."""
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
logs=Path('sdk-rs/target/binance-margin-dispose')/sha
logs.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
inputs=[crate/'src/exchange_dispose.rs',crate/'src/margin_mode.rs',crate/'src/lib.rs',crate/'tests/margin_dispose.rs',crate/'scripts/margin-dispose.mjs',Path(__file__).relative_to(root)]
inputs+=list(Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.exchange.ts.toml'))
inputs+=list(Path('packages/safe-cex/src/exchanges/binance').glob('*.exchange.ts'))
inputs+=[Path('packages/safe-cex/src/exchanges/binance/binance.types.ts')]
originals={p:p.read_bytes() for p in inputs}
for p,content in originals.items():assert content==subprocess.check_output(['git','show',f'{sha}:{p}']),p
records=[]
def run(label,cmd,exit_code=0,assertion=None):
    p=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (logs/(label+'.log')).write_bytes(p.stdout)
    text=p.stdout.decode(errors='replace')
    record=dict(label=label,gate_sha=sha,command=cmd,exit=p.returncode,log=str(logs/(label+'.log')),log_sha256=hashlib.sha256(p.stdout).hexdigest(),output=text)
    if assertion:record['named_native_failure']=assertion
    records.append(record)
    print(f'{label}: exit {p.returncode}',flush=True)
    assert p.returncode==exit_code,text
    if assertion:assert 'Finished `test` profile' in text and assertion in text and 'test result: FAILED. 0 passed; 1 failed;' in text,text
    elif label in ['baseline','final-restore']:assert 'test result: ok. 5 passed; 0 failed; 0 ignored;' in text,text
    elif label=='extractor-check':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in text,text
cargo=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--test','margin_dispose','--jobs','2']
run('baseline',cargo+['--','--include-ignored'])
controls=[
 ('futures-public-private-swap','src/exchange_dispose.rs','dispose_binance_exchange','dispose_public()?;\n    dispose_private()','dispose_private()?;\n    dispose_public()','futures_disposal_matches_source','DISPOSE binance case 0 native order','[Base, Private, Public] versus [Base, Public, Private]'),
 ('pm-public-private-swap','src/exchange_dispose.rs','dispose_binance_pm_exchange','dispose_public()?;\n    dispose_private()','dispose_private()?;\n    dispose_public()','pm_disposal_matches_source','DISPOSE binancepm case 0 native order','[Base, Private, Public] versus [Base, Public, Private]'),
 ('spot-public-private-swap','src/exchange_dispose.rs','dispose_binance_spot_exchange','dispose_public()?;\n    dispose_private()','dispose_private()?;\n    dispose_public()','spot_disposal_matches_source','DISPOSE binancespot case 0 native order','[Base, Private, Public] versus [Base, Public, Private]'),
 ('futures-crossed-for-isolated','src/margin_mode.rs','change_binance_margin_type','                BinanceMarginType::Isolated\n','                BinanceMarginType::Crossed\n','futures_margin_type_matches_source','MARGIN case 0 native margin type','Crossed versus Isolated'),
]
for label,relative,function,before,after,test,assertion,pair in controls:
    path=crate/relative
    original=originals[path].decode()
    # Limit replacements to this actual function so each row gets its own control.
    start=original.index('pub '+('async fn ' if function=='change_binance_margin_type' else 'fn ')+function+'<')
    end=original.find('\n// ts:',start)
    if end<0:end=len(original)
    body=original[start:end]
    assert body.count(before)==1,(label,before)
    try:
        path.write_text(original[:start]+body.replace(before,after)+original[end:])
        run(label,cargo+['--','--include-ignored',test,'--exact'],101,assertion)
        records[-1].update(call_site=str(path)+'::'+function,mutation_before=before,mutation_after=after,native_pair=pair)
    finally:path.write_bytes(originals[path])
    assert path.read_bytes()==originals[path]
run('final-restore',cargo+['--','--include-ignored'])
run('scoped-cargo-check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'])
run('extractor-check',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'])
run('scoped-rustfmt',['rustfmt','--edition','2024','--check',str(crate/'src/exchange_dispose.rs'),str(crate/'src/margin_mode.rs'),str(crate/'tests/margin_dispose.rs')])
run('git-diff-check',['git','diff','--check'])
for p,content in originals.items():assert p.read_bytes()==content,p

def census(revision):
    rows={}
    for p in Path('sdk-rs/parity/manifest/exchanges/binance').rglob('*.toml'):
        for row in tomllib.loads(subprocess.check_output(['git','show',f'{revision}:{p}']).decode())['symbol']:
            key=(str(p),row['ts'],row['kind'])
            assert key not in rows,key
            rows[key]=row['status']
    return rows
before,after=census(base),census(sha)
assert before.keys()==after.keys()
changed=[dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changed)==4 and all(r['before']=='pending' and r['after']=='ported' for r in changed)
counts=lambda rows:{s:list(rows.values()).count(s) for s in ['ported','divergent','pending']}
evidence=dict(gate_sha=sha,parent_built_on=base,unreviewed_work_in_base=False,moved_pending_to_ported=4,moved_pending_to_divergent=0,census_before=counts(before),census_after=counts(after),changed_rows=changed,records=records,restored_inputs_sha256={str(p):hashlib.sha256(v).hexdigest() for p,v in originals.items()},controls_surviving=[],enumeration_floor='Completed all 68 authored source scenarios: 8 disposal per family (24 total) and 44 futures margin cases, plus two pending POST polling cases. Stopped after four selected compiling controls, one per new row. No exhaustive input or whole-venue proof claim.',scope='Three synchronous disposal bodies with configured base/public/private callbacks and original generic error identity; concrete class/socket/arrow binding remains separate composition. Futures margin uses configured POST/emitter, normalized ExchangeError, typed bool/string arguments and JSON messages. Executable getters/coercions, synchronous callback throws and arbitrary emitter mutation of rejected objects remain outside the declared margin callback boundary.',environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),preliminary_uncommitted_test=dict(exit=0,passed=5,failed=0,ignored=0,scope='before candidate commit; not substituted for final gates'),manifest_writer=dict(command=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract'],exit=0,output='Updated 3 manifests; 0 added, 0 orphaned symbols.',scope='uncommitted row changes based on requested parent before candidate commit'),not_run=['full crate test suite','workspace compilation','host clippy','wasm clippy','CI reruns','concrete exchange/client/socket composition','whole-venue audit','cross-engine error wording verification'])
(crate/'MARGIN_DISPOSE_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n')
print('Evidence recorded at '+str(crate/'MARGIN_DISPOSE_GATE_EXITS.json'),flush=True)
