"""Serial, SHA-bound differential observations and compiled mutation controls."""
from pathlib import Path
from datetime import datetime, timezone
import argparse, hashlib, json, os, subprocess, time, tomllib

ROOT=Path(__file__).resolve().parents[3]
EVID=ROOT/'sdk-rs/parity/evidence/polymarket-oracle-classes'
TESTS=ROOT/'sdk-rs/safe-cex-venue-polymarket/tests'
SRC=ROOT/'sdk-rs/safe-cex-venue-polymarket/src'
SAMPLES=[('closed','closed','class_cost_closed_native'),('scalar','utilities','class_cost_normalize_native'),('scalar','utilities','class_cost_precision_native'),('collection','utilities','class_cost_tokens_native'),('record','record','class_cost_record_native'),('http','http','class_cost_http_native'),('orchestration','venue','class_cost_markets_native'),('orchestration','venue','class_cost_tickers_native'),('orchestration','venue','class_cost_place_native')]

def stamp():return datetime.now(timezone.utc).isoformat()
def sha():return subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def env():
    result=os.environ.copy()
    if not result.get('SAFE_CEX_TYPESCRIPT_PATH'):
        common=Path(subprocess.check_output(['git','rev-parse','--git-common-dir'],cwd=ROOT,text=True).strip()).resolve()
        result['SAFE_CEX_TYPESCRIPT_PATH']=str(common.parent/'node_modules/typescript')
    return result
def input_hashes():
    paths=list(SRC.rglob('*.rs'))+list(TESTS.glob('class_cost*'))+[TESTS/'eager_sdk_source.cjs',TESTS/'public_reads_oracle.cjs',TESTS/'exchange.rs',TESTS/'exchange_oracle.cjs',TESTS/'support/completion.rs']
    paths+=list((ROOT/'packages/safe-cex/src/exchanges/polymarket').glob('*.ts'))+[ROOT/'packages/safe-cex/src/utils/safe-math.ts',ROOT/'packages/math/src/isNotNumber.ts',ROOT/'sdk-rs/Cargo.lock']
    return {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths if p.is_file()}
def clean(text):return text.replace(str(ROOT),'<worktree>').replace(str(Path.home()),'<home>')
def run(label,command,metadata=None,stdin=None):
    EVID.mkdir(parents=True,exist_ok=True)
    assert not os.environ.get('POLYMARKET_SOURCE_CONTROL'),'external source mutation flag must be absent'
    pinned=sha();inputs=input_hashes()
    allowed=(metadata or {}).get('source') if (metadata or {}).get('mutation') else None
    changed=subprocess.check_output(['git','diff','HEAD','--name-only'],cwd=ROOT,text=True).splitlines()
    dirty=[p for p in changed if not p.startswith('sdk-rs/parity/evidence/polymarket-oracle-classes/')]
    assert dirty==([allowed] if allowed else []),('dirty measurement inputs',dirty,allowed)
    if allowed:
        original=subprocess.check_output(['git','show',pinned+':'+allowed],cwd=ROOT)
        assert hashlib.sha256(original).hexdigest()==metadata['original_sha256']
        assert hashlib.sha256((ROOT/allowed).read_bytes()).hexdigest()==metadata['mutated_sha256']
    prior=EVID/(label+'.json')
    if prior.exists():
        old=json.loads(prior.read_text()); archive=EVID/'attempt-history';archive.mkdir(exist_ok=True)
        suffix=old['sha'][:10]+'-'+old['started_utc'].replace(':','-')
        for extension in ('json','log'):
            source=EVID/(label+'.'+extension)
            if source.exists():(archive/(label+'-'+suffix+'.'+extension)).write_bytes(source.read_bytes())
    started=stamp();start=time.monotonic()
    result=subprocess.run(command,cwd=ROOT/'sdk-rs' if command[0]=='cargo' else ROOT,env=env(),input=stdin,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    elapsed=time.monotonic()-start
    assert sha()==pinned,'HEAD changed during measured command'
    assert input_hashes()==inputs,'measured source bytes changed during command'
    after=subprocess.check_output(['git','diff','HEAD','--name-only'],cwd=ROOT,text=True).splitlines()
    assert [p for p in after if not p.startswith('sdk-rs/parity/evidence/polymarket-oracle-classes/')]==dirty,'input paths changed during command'
    if allowed: assert hashlib.sha256((ROOT/allowed).read_bytes()).hexdigest()==metadata['mutated_sha256']
    log=clean(result.stdout);file=EVID/(label+'.log');file.write_text(log)
    record={'label':label,'sha':pinned,'non_evidence_dirty_inputs':dirty,'input_sha256':inputs,'command':command,'started_utc':started,'finished_utc':stamp(),'wall_seconds':elapsed,'exit':result.returncode,'log_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),**(metadata or {})}
    (EVID/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
    print(label,result.returncode,round(elapsed,3),flush=True)
    if result.returncode:print(log[-6000:],flush=True)
    return record,log
def cargo(target,test):
    if target=='http':return ['cargo','test','-p','safe-cex-venue-polymarket','--locked','--jobs','2','--lib','--','binding::exchange::eager_tests::'+test,'--exact','--nocapture']
    return ['cargo','test','-p','safe-cex-venue-polymarket','--locked','--jobs','2','--test','class_cost_'+target,'--',test,'--exact','--nocapture']
def baseline():
    for cls,target,test in SAMPLES:
        row,_=run('baseline-'+test,cargo(target,test),{'class':cls,'test':test,'cases':512})
        if row['exit']:raise SystemExit(row['exit'])
def runtime():
    # The command and its compilation/no-op Cargo overhead are part of this price.
    for repetition in range(3):
        for cls,target,test in SAMPLES:
            row,_=run(f'warm-{repetition}-{test}',cargo(target,test),{'class':cls,'test':test,'repetition':repetition,'cases':512})
            if row['exit']:raise SystemExit(row['exit'])
def census():
    rows=[]
    for path in sorted((ROOT/'sdk-rs/parity/manifest').rglob('*.toml')):
        data=tomllib.loads(path.read_text())
        rows.extend({'source':data['file']['ts'],'ts':r['ts'],'kind':r['kind'],'status':r['status'],'manifest':str(path.relative_to(ROOT))} for r in data.get('symbol',[]))
    assert len(rows)==7341
    (EVID/'inventory.json').write_text(json.dumps(rows,indent=2)+'\n')
    output=subprocess.run(['node',str(TESTS/'class_cost_census.cjs')],cwd=ROOT,env=env(),input=json.dumps(rows),capture_output=True,text=True,check=True)
    data=json.loads(output.stdout)
    (EVID/'census.json').write_text(json.dumps({'sha':sha(),**data},indent=2)+'\n')
    print(data['counts'],flush=True)
def controls():
    mutations=[
      ('name','spot_contracts.rs','    ExchangeName::Polymarket','    ExchangeName::Mexc','closed','class_cost_closed_native','class_cost_closed_native'),
      ('tick-membership','utilities.rs','["0.1", "0.01", "0.005",','["0.1", "0.01", "0.003",','utilities','class_cost_normalize_native','generated_tick_native'),
      ('precision-default','utilities.rs','return 2.0;','return 3.0;','utilities','class_cost_precision_native','generated_precision_native'),
      ('token-identity','utilities.rs','Some((token_id, outcome))','Some((outcome, token_id))','utilities','class_cost_tokens_native','generated_tokens_native_token'),
      ('record-identity','types.rs','price: fields.price,\n            size: fields.size,','price: fields.size,\n            size: fields.price,','record','class_cost_record_native','class_cost_record_price'),
      ('http-endpoint','binding/members.rs','"getServerTime" => endpoint("TIME")?','"getServerTime" => endpoint("MARKETS")?','http','class_cost_http_native','class_cost_http_url'),
      ('market-quote','exchange.rs','h.text("pUSD")','h.text("USDC")','venue','class_cost_markets_native','class_cost_native'),
      ('ticker-batch','exchange.rs','(0..n).step_by(100)','(0..n).step_by(101)','venue','class_cost_tickers_native','class_cost_native'),
      ('placement-side','exchange.rs','h.text("BUY")','h.text("SELL")','venue','class_cost_place_native','class_cost_native'),
      ('source-map-erasure','../tests/exchange_oracle.cjs',"if(Object.prototype.toString.call(v)==='[object Map]')",'if(v instanceof Map)','venue','class_cost_markets_native','class_cost_native_fields'),
      ('token-length17','utilities.rs','    token_ids\n        .into_iter()','    if token_ids.len() == 17 { return Vec::new(); }\n    token_ids\n        .into_iter()','utilities','class_cost_tokens_native','generated_tokens_native_length'),
      ('token-length65','utilities.rs','    token_ids\n        .into_iter()','    if token_ids.len() == 65 { return Vec::new(); }\n    token_ids\n        .into_iter()','utilities','class_cost_tokens_native','generated_tokens_native_length'),
    ]
    assert len(mutations)==12,'independent control floor'
    findings=[];outcomes=[]
    for label,file,before,after,target,test,assertion in mutations:
        assert assertion,'every selected control must require a named runtime failure'
        path=(SRC/file).resolve();original=path.read_bytes();assert original==subprocess.check_output(['git','show',sha()+':'+str(path.relative_to(ROOT))],cwd=ROOT);text=original.decode();assert text.count(before)==1,(label,text.count(before))
        try:
            path.write_text(text.replace(before,after))
            row,log=run('mutant-'+label,cargo(target,test),{'test':test,'mutation':label,'source':str(path.relative_to(ROOT)),'original_sha256':hashlib.sha256(original).hexdigest(),'mutated_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'expected_assertion':assertion})
        finally:path.write_bytes(original)
        restored,restore_log=run('restore-'+label,cargo(target,test),{'test':test,'restored_sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
        displayed='binding::exchange::eager_tests::'+test if target=='http' else test
        detected=(row['exit']==101 and assertion in log and 'panicked at' in log and
                  f'test {displayed} ... FAILED' in log and 'test result: FAILED. 0 passed; 1 failed;' in log and 'could not compile' not in log)
        restored_pass=(restored['exit']==0 and f'test {displayed} ... ok' in restore_log and 'test result: ok. 1 passed; 0 failed;' in restore_log)
        outcomes.append({'mutation':label,'family':'source encoder' if label=='source-map-erasure' else 'Rust','expected_assertion':assertion,'detected':detected,'mutant_exit':row['exit'],'restored_pass':restored_pass,'restore_exit':restored['exit'],'sha':row['sha']})
        if not detected or not restored_pass:
            findings.append({'type':'coverage' if row['exit']==0 else 'control-failure','mutation':label,'source':str(path.relative_to(ROOT)),'mutant_exit':row['exit'],'restore_exit':restored['exit'],'reason':'Selected fault did not produce its named compiled runtime failure, or restoration failed.'})
    (EVID/'survivors.json').write_text(json.dumps(findings,indent=2)+'\n')
    summary={'sha':sha(),'selected_controls':12,'detected':sum(r['detected'] for r in outcomes),'survivors':sum(r['mutant_exit']==0 for r in outcomes),'restored_passes':sum(r['restored_pass'] for r in outcomes),'stopped_after_control_index':11,'outcomes':outcomes}
    (EVID/'control-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    assert not findings,findings
    print('CONTROL FLOOR COMPLETE:12 detected,12 restored,0 survivors; stop index11',flush=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['baseline','runtime','census','controls']);parser.add_argument('--evidence-dir',type=Path);args=parser.parse_args()
    if args.evidence_dir:
        selected=(ROOT/args.evidence_dir).resolve()
        assert selected.is_relative_to(EVID.resolve()),'evidence directory must stay inside this experiment'
        EVID=selected
    globals()[args.mode]()
