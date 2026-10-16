"""One compiled production mutation per newly ported submission validator."""
from pathlib import Path
import json, os, subprocess

crate = Path(__file__).resolve().parents[1]
source = crate/'src/submit_validation.rs'
logs = Path(os.environ['MEXC_VALIDATION_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/mexc')],cwd=crate,text=True).strip(), 'commit lane inputs first'
controls = [
    ('single-response-alias', 'mexc_single_submit_validation_matches_source', 'MEXC native original response handle', 'result.response_data = Some(payload.clone());', 'result.response_data = Some(Rc::new(RefCell::new(payload.borrow().clone())));'),
    ('batch-failure-index', 'mexc_batch_submit_validation_matches_source', 'MEXC native first failure index', 'result.failed_order_index = Some(failed_rows[0].0);', 'result.failed_order_index = Some(failed_rows[0].0 + 1);'),
]
assert len(controls) == 2 and controls[-1][0] == 'batch-failure-index'
original = source.read_text()
summary = {'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(), 'floor':'two validators; stopped after batch-failure-index', 'controls':[]}
def run(name, test):
    command = ['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-mexc','--test','submit_validation','--jobs','2','--locked','--','--exact',test,'--nocapture']
    result = subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={result.returncode}\n'+result.stdout)
    return result
for name,test,assertion,old,new in controls:
    assert original.count(old) == 1, name
    baseline = run(name+'-baseline',test)
    assert baseline.returncode == 0, baseline.stdout
    try:
        source.write_text(original.replace(old,new))
        mutation = run(name+'-mutation',test)
        caught = mutation.returncode == 101 and assertion in mutation.stdout
    finally:
        source.write_text(original)
    restored = run(name+'-restored',test)
    summary['controls'].append({'name':name,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':mutation.returncode,'restored_exit':restored.returncode,'caught':caught})
    (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    assert restored.returncode == 0, restored.stdout
    assert caught, f'FINDING survivor or wrong failure: {name}\n{mutation.stdout}'
    print(f'{name}: 0 / 101 {assertion} / restored 0',flush=True)
