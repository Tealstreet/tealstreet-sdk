"""Three serial shipping gates; every exit belongs to the pinned input SHA."""
from pathlib import Path
from datetime import datetime,timezone
import argparse,hashlib,importlib.util,json,subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
spec=importlib.util.spec_from_file_location('cost_driver',ROOT/'sdk-rs/safe-cex-venue-polymarket/tests/class_cost.py')
driver=importlib.util.module_from_spec(spec);spec.loader.exec_module(driver)
parser=argparse.ArgumentParser();parser.add_argument('--evidence-dir',type=Path,default=HERE);args=parser.parse_args()
OUTPUT=(ROOT/args.evidence_dir).resolve()
assert OUTPUT.is_relative_to(HERE),'gate receipts must stay inside this experiment'
OUTPUT.mkdir(parents=True,exist_ok=True)
driver.EVID=OUTPUT
sha=driver.sha()
runner=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
commands=[
 ('workspace-fmt',['cargo','fmt','--manifest-path','sdk-rs/Cargo.toml','--all','--','--check']),
 ('workspace-build',['cargo','build','--manifest-path','sdk-rs/Cargo.toml','--workspace','--locked','--jobs','2']),
 ('host-workspace-clippy',['cargo','clippy','--manifest-path','sdk-rs/Cargo.toml','--workspace','--all-targets','--locked','--jobs','2']),
]
receipt={'sha':sha,'branch':subprocess.check_output(['git','branch','--show-current'],cwd=ROOT,text=True).strip(),'started_utc':datetime.now(timezone.utc).isoformat(),'gate_runner_sha256':runner,'runs':[],'not_run':['full workspace/package tests, legacy exchange target and other older oracle targets at this campaign SHA','canonical wasm clippy (central owner), standalone wasm build and browser/FFI execution','CI jobs, routing/layers/rust-ci-wiring, extractor/parity/static report checks and other venue oracles']}
for name,command in commands:
    assert driver.sha()==sha
    assert hashlib.sha256(Path(__file__).read_bytes()).hexdigest()==runner
    print('START GATE',name,sha,flush=True)
    measured,_=driver.run('gate-'+name,command,{'phase':'workspace-gate','gate_runner_sha256':runner})
    receipt['runs'].append(measured)
    receipt['latest_observed_utc']=datetime.now(timezone.utc).isoformat()
    (OUTPUT/f'gate-summary-{sha[:10]}.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if measured['exit']:raise SystemExit(measured['exit'])
receipt['finished_utc']=datetime.now(timezone.utc).isoformat()
(OUTPUT/f'gate-summary-{sha[:10]}.json').write_text(json.dumps(receipt,indent=2)+'\n')
print('THREE GATES COMPLETE',sha,flush=True)
