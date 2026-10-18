"""Nine compiled native controls, exact restores, and named failure sites."""
import json, os, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
LOGS=ROOT/'.aimux/bybit-variants';LOGS.mkdir(parents=True,exist_ok=True)
SOURCE=ROOT/'sdk-rs/safe-cex-venue-bybit/src/hyro_private_session.rs'
COMMAND=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','hyro_private_session','--jobs','2']
ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
original=SOURCE.read_text();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
controls=[
 ('class-ready','h.define(&owner, "ready", h.boolean(false))?;','h.define(&owner, "ready", h.boolean(true))?;','class_native_initial_ready'),
 ('session-version','h.get(owner, "generation")\n}','h.get(owner, "stateRevision")\n}','version_native_generation'),
 ('recovery-complete','Ok(h.boolean(true))\n    } else {\n        h.get(owner, "ready")','Ok(h.boolean(false))\n    } else {\n        h.get(owner, "ready")','recovery_native_complete'),
 ('constructor-observer','call!(\n                h,\n                h.get(owner, "parent")?,\n                "observePrivateOrders",\n                vec![first.clone()]\n            );','let _retained_rows = first.clone();','constructor_native_order_observer'),
 ('reset-ready','h.set(owner, "ready", h.boolean(false))?;','h.set(owner, "ready", h.boolean(true))?;','reset_native_ready_false'),
 ('resync-getter','pub fn resyncs_private_state_on_reopen<H: SessionHost>(h: &H) -> H::Value {\n    h.boolean(false)','pub fn resyncs_private_state_on_reopen<H: SessionHost>(h: &H) -> H::Value {\n    h.boolean(true)','resync_native_false'),
 ('reconnect-ready','h.set(owner, "ready", h.boolean(true))?;','h.set(owner, "ready", h.boolean(false))?;','reconnect_native_ready'),
 ('request-timeout','vec![callback, h.number(10_000.0)]','vec![callback, h.number(10_001.0)]','request_native_timeout_delay'),
]
def run(label):
 r=subprocess.run(COMMAND,cwd=ROOT,env=ENV,capture_output=True,text=True);out=r.stdout+r.stderr;(LOGS/f'hyro-control-{label}.log').write_text(out);return r.returncode,out
baseline,out=run('baseline');assert baseline==0,out
report=dict(head=head,baseline_exit=baseline,controls=[],survivors=[])
for name,needle,replacement,assertion in controls:
 assert original.count(needle)==1,(name,'nonunique or missing site',original.count(needle))
 try:
  SOURCE.write_text(original.replace(needle,replacement));mutated,out=run(name)
 finally:SOURCE.write_text(original)
 record=dict(control=name,assertion=assertion,mutation_exit=mutated,compiled_runtime_failure=mutated==101 and 'test result: FAILED' in out)
 if mutated==0:report['survivors'].append(name)
 report['controls'].append(record);(LOGS/'hyro-controls.json').write_text(json.dumps(report,indent=2)+'\n')
 assert record['compiled_runtime_failure'] and assertion in out,(name,out)
 restored,out=run(name+'-restored');record['restored_exit']=restored;(LOGS/'hyro-controls.json').write_text(json.dumps(report,indent=2)+'\n');assert restored==0,(name,out)
 assert SOURCE.read_text()==original;print(json.dumps(record),flush=True)
assert len(report['controls'])==8
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==head
