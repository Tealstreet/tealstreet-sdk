"""Compiled native controls, sequential; exact restore even on interruption."""
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import sys
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src/order_execution.rs'
SHA = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
subprocess.run(['git','diff','--exit-code','HEAD','--','sdk-rs/safe-cex-venue-asterdex/src','sdk-rs/safe-cex-venue-asterdex/tests','sdk-rs/safe-cex-venue-asterdex/scripts','sdk-rs/parity/manifest/exchanges/asterdex'], cwd=ROOT, check=True)
BASELINES={SOURCE:SOURCE.read_bytes()}
CONTROLS=[
('validation',SOURCE,'error:"".into(),balance:None','error:"mutated".into(),balance:None','validate-v1 native validation result'),
('placement',SOURCE,'Ok(vec![id])','Ok(vec!["mutated".into()])','limit-rounded-builder-maker native returned IDs'),
('interest-update',SOURCE,'patch.set_number("openInterest", n);','patch.set_number("openInterest", n+1.0);','interest-direct-unchanged-missing native ticker state'),
('interest-lifecycle',SOURCE,'self.owner.start_timeout(15000,','self.owner.start_timeout(15001,','interest-bulk native timer deadlines'),
]
assert len(CONTROLS)==4
ARGV = ['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-asterdex','--test','live_order_execution','--jobs','2','--','--include-ignored','--nocapture']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
record = {'input_sha': SHA, 'baseline_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(data).hexdigest() for path,data in BASELINES.items()}, 'iteration': 1, 'planned': 4, 'argv': ARGV, 'controls': [], 'survivors': []}
def save():
    pathlib.Path('/tmp/asterdex-order-controls.json').write_text(json.dumps(record,indent=2)+'\n')
def interrupted(_signal, _frame):
    raise KeyboardInterrupt('control interrupted; restoring production bytes')
signal.signal(signal.SIGTERM, interrupted)
def run(argv):
    with subprocess.Popen(argv,cwd=ROOT,env=ENV,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True) as process:
        try:
            output, _ = process.communicate()
        except BaseException:
            os.killpg(process.pid, signal.SIGTERM)
            process.communicate()
            raise
    print(output,flush=True)
    return process.returncode, output
try:
    for family, path, original, replacement, assertion in CONTROLS:
        baseline = BASELINES[path]
        text = baseline.decode()
        assert text.count(original) == 1, (family, 'mutation site not unique')
        path.write_text(text.replace(original,replacement,1))
        failed_exit, failed_output = run(ARGV)
        path.write_bytes(baseline)
        restored_exit, restored_output = run(ARGV)
        caught = failed_exit == 101 and 'test result: FAILED' in failed_output and assertion in failed_output and 'could not compile' not in failed_output and restored_exit == 0 and 'test result: ok' in restored_output
        record['controls'].append({'family':family,'call_site':str(path.relative_to(ROOT)),'original':original,'replacement':replacement,'assertion':assertion,'mutation_exit':failed_exit,'restored_exit':restored_exit,'caught':caught,'mutation_output':failed_output,'restored_output':restored_output})
        if not caught: record['survivors'].append({'family':family,'call_site':str(path.relative_to(ROOT)), 'assertion':assertion})
        record['stopped_after']=family;save()
        print('CONTROL',family,'101->0' if caught else 'FINDING',flush=True)
        if not caught: sys.exit(1)
finally:
    for path,baseline in BASELINES.items(): path.write_bytes(baseline)
    record['restored_exact_bytes']=all(path.read_bytes()==baseline for path,baseline in BASELINES.items())
    save()
