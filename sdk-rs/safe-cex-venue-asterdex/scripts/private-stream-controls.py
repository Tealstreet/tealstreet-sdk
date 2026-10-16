"""Compiled native controls, sequential; exact restore even on interruption."""
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import sys
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src/private_stream.rs'
CORE = ROOT / 'sdk-rs/safe-cex-exchange/src/websocket.rs'
SHA = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
subprocess.run(['git','diff','--exit-code','HEAD','--','sdk-rs/safe-cex-venue-asterdex/src','sdk-rs/safe-cex-venue-asterdex/tests','sdk-rs/safe-cex-venue-asterdex/scripts','sdk-rs/safe-cex-exchange/src','sdk-rs/parity/manifest/exchanges/asterdex'], cwd=ROOT, check=True)
BASELINES={SOURCE:SOURCE.read_bytes(),CORE:CORE.read_bytes()}
CONTROLS=[
('lifecycle',CORE,'            before_close();','            let _ = before_close;','private lifecycle construct-close-dispose step 2 timers','private_lifecycle_matches_source'),
('connection',SOURCE,'{}/ws/{key}','{}/bad/{key}','private connection connect-production exact venue logs','private_connection_matches_source'),
('dispatch',SOURCE,'        self.socket.track_last_received()?;','        let _ = &self.socket;','private dispatch receive','private_dispatch_matches_source'),
('orders',SOURCE,'js_truthy(d.get("R")) || js_truthy(d.get("cp"))','js_truthy(d.get("R")) || false','private orders new-close-position step 0 reduceOnly','private_orders_match_source'),
('balance',SOURCE,'next.upnl = current.upnl + delta("USDT");','next.upnl = current.upnl + delta("USDT") + 1.0;','private balance balance-single-asset step 0 upnl','private_balance_matches_source'),
('account',SOURCE,'self.last_account_event_time.set(time);','self.last_account_event_time.set(time + 1.0);','private account account-updates-before-balance step 0 event ordering','private_account_matches_source'),
]
assert len(CONTROLS)==6
ARGV = ['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-asterdex','--test','live_private_stream','--jobs','2','--','--include-ignored','--nocapture']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
record = {'input_sha': SHA, 'baseline_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(data).hexdigest() for path,data in BASELINES.items()}, 'iteration': 2, 'planned': 6, 'argv': ARGV, 'controls': [], 'survivors': []}
def save():
    pathlib.Path('/tmp/asterdex-private-controls.json').write_text(json.dumps(record,indent=2)+'\n')
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
    for family, path, original, replacement, assertion, test in CONTROLS:
        baseline = BASELINES[path]
        text = baseline.decode()
        assert text.count(original) == 1, (family, 'mutation site not unique')
        path.write_text(text.replace(original,replacement,1))
        failed_exit, failed_output = run(ARGV + [test, "--exact"])
        path.write_bytes(baseline)
        restored_exit, restored_output = run(ARGV + [test, "--exact"])
        caught = failed_exit == 101 and 'test result: FAILED' in failed_output and assertion in failed_output and 'could not compile' not in failed_output and restored_exit == 0 and 'test result: ok' in restored_output
        record['controls'].append({'family':family,'call_site':str(path.relative_to(ROOT)),'original':original,'replacement':replacement,'assertion':assertion,'test':test,'mutation_exit':failed_exit,'restored_exit':restored_exit,'caught':caught,'mutation_output':failed_output,'restored_output':restored_output})
        if not caught: record['survivors'].append({'family':family,'call_site':str(path.relative_to(ROOT)), 'assertion':assertion})
        record['stopped_after']=family;save()
        print('CONTROL',family,'101->0' if caught else 'FINDING',flush=True)
        if not caught: sys.exit(1)
finally:
    for path,baseline in BASELINES.items(): path.write_bytes(baseline)
    record['restored_exact_bytes']=all(path.read_bytes()==baseline for path,baseline in BASELINES.items())
    save()
