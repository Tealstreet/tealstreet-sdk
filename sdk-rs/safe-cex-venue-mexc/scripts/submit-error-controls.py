"""One production control per newly ported submit-error helper."""
from pathlib import Path
import json, os, subprocess

crate = Path(__file__).resolve().parents[1]
source = crate/'src/exchange_helpers.rs'
logs = Path(os.environ['MEXC_SUBMIT_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/mexc')],cwd=crate,text=True).strip(), 'commit lane inputs first'
controls = [
    ('batch-denial', 'mexc_native_batch_access_denial_matches_source', 'MEXC native batch denial:', 'code == 511.0', 'code == 512.0'),
    ('submit-cooldown', 'mexc_submit_cooldown_matches_source', 'MEXC submit cooldown:', 'now() + 1_500.0', 'now() + 1_499.0'),
]
assert len(controls) == 2 and controls[-1][0] == 'submit-cooldown'
original = source.read_text()
summary = {'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(), 'floor':'two rows; stopped after submit-cooldown', 'controls':[]}
def run(name, test):
    command = ['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-mexc','--test','submit_errors','--jobs','2','--locked','--','--exact',test,'--nocapture']
    result = subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={result.returncode}\n'+result.stdout)
    return result
for name,test,assertion,old,new in controls:
    assert original.count(old) == 1, old
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
