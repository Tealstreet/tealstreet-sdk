"""Only current fmt/build gates, serial and pinned to the handoff commit."""
from pathlib import Path
from datetime import datetime,timezone
import importlib.util,json,hashlib,subprocess
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[4]
spec=importlib.util.spec_from_file_location('resolution_driver',ROOT/'sdk-rs/safe-cex-venue-polymarket/tests/resolution_cost.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);d=module.d
sha=d.sha();runner=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
commands=[('workspace-fmt',['cargo','fmt','--manifest-path','sdk-rs/Cargo.toml','--all','--','--check']),('workspace-build',['cargo','build','--manifest-path','sdk-rs/Cargo.toml','--workspace','--locked','--jobs','2'])]
receipt={'sha':sha,'branch':subprocess.check_output(['git','branch','--show-current'],cwd=ROOT,text=True).strip(),'runner_sha256':runner,'started_utc':datetime.now(timezone.utc).isoformat(),'runs':[],'not_run':['host clippy and canonical wasm clippy (central), standalone wasm build or browser execution','full workspace/package tests, legacy oracle targets and other venues','extractor/static/report checks, CI wiring/layers/routing, live services']}
for label,command in commands:
 assert d.sha()==sha and hashlib.sha256(Path(__file__).read_bytes()).hexdigest()==runner
 print('START',label,sha,flush=True)
 r,log=d.run('gate-'+label,command,{'phase':'shipping-gate','runner_sha256':runner})
 receipt['runs'].append(r)
 (HERE/f'gate-summary-{sha[:10]}.json').write_text(json.dumps(receipt,indent=2)+'\n')
 if r['exit']:raise SystemExit(r['exit'])
receipt['finished_utc']=datetime.now(timezone.utc).isoformat()
(HERE/f'gate-summary-{sha[:10]}.json').write_text(json.dumps(receipt,indent=2)+'\n')
print('TWO REQUIRED GATES COMPLETE',sha,flush=True)
