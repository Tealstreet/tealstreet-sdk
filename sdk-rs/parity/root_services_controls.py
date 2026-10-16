"""Eight discriminating production controls; always restore bytes, serial cargo only.

Writes measured exits/logs bound to the pre-control HEAD. Run from the worktree root.
Enumeration stops after control eight; this is not an exhaustive mutation census.
"""
import hashlib,json,os,subprocess
from pathlib import Path
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
out=Path('sdk-rs/parity/root_services_evidence');out.mkdir(exist_ok=True)
controls=[
 ('ws-secret', 'safe-cex-store', 'diagnostics', 'ws_redaction_native_fields_and_fallbacks', 'sdk-rs/safe-cex-store/src/ws_diagnostics.rs', 'const REDACTED: &str = "[redacted]";', 'const REDACTED: &str = "visible";'),
 ('rest-sign-ms','safe-cex-store','diagnostics','rest_native_stamps_aggregates_and_bounds','sdk-rs/safe-cex-store/src/venue_rest_diagnostics.rs','insert("safeCexRestDiagSignMs".into(), sign_ms);','insert("safeCexRestDiagSignMs".into(), sign_ms + 1.0);'),
 ('rest-write-prune','safe-cex-store','diagnostics','rest_native_stamps_aggregates_and_bounds','sdk-rs/safe-cex-store/src/venue_rest_diagnostics.rs','diagnostics.timestamps.retain(|at| *at >= cutoff);','diagnostics.timestamps.retain(|_| true);'),
 ('metadata-decoder-swap','safe-cex-types','exchange_metadata','full_metadata_native_fields_unified_mobile_and_domains','sdk-rs/safe-cex-types/src/exchange_metadata_generated.rs','#[serde(rename = "requiresCors")]','#[serde(rename = "forbidCors")]'),
 ('token-context-ownership','safe-cex-exchange','access_token','contextual_token_owns_call_and_preserves_error_identity','sdk-rs/safe-cex-exchange/src/access_token.rs','self.get_contextual_proxy_token().await?','None'),
 ('factory-cache','safe-cex','root_services','factory_cache_native_options_and_store_identity','sdk-rs/safe-cex/src/factory.rs','self.cache.borrow().get(name).cloned()','None::<ExchangeConstructor<I>>'),
 ('factory-forced-disable','safe-cex','root_services','factory_cache_native_options_and_store_identity','sdk-rs/safe-cex/src/factory.rs','options.tealstreet_proxy = Some(false);','options.tealstreet_proxy = Some(true);'),
 ('account-broker-clear','safe-cex','root_services','account_native_forwarding_overrides_and_broker_clear','sdk-rs/safe-cex/src/account.rs','apply_extra_patch(patch, &mut extra);','/* deliberately ignore explicit broker clear */'),
]
results=[]
for name,crate,test,assertion,file,needle,replacement in controls:
 path=Path(file);original=path.read_bytes();text=original.decode();assert needle in text,(name,needle)
 mutated=text.replace(needle,replacement,1)
 if name=='metadata-decoder-swap':
  # Symmetric decoder/encoder swap: the explicit native GRVT assertion must see it.
  mutated=text.replace('#[serde(rename = "requiresCors")]','#[serde(rename = "SWAP")]').replace('#[serde(rename = "forbidCors")]','#[serde(rename = "requiresCors")]').replace('#[serde(rename = "SWAP")]','#[serde(rename = "forbidCors")]')
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p',crate,'--test',test,'--jobs','2',assertion,'--','--exact']
 print('CONTROL',name,'at',sha,flush=True)
 try:
  path.write_text(mutated)
  run=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (out/(name+'.broken.log')).write_text(run.stdout)
  caught=run.returncode==101 and ('test '+assertion+' ... FAILED') in run.stdout
  results.append({'control':name,'sha':sha,'file':file,'assertion':assertion,'broken_exit':run.returncode,'caught_named_assertion':caught,'original_sha256':hashlib.sha256(original).hexdigest()})
 finally:path.write_bytes(original)
 restored=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (out/(name+'.restored.log')).write_text(restored.stdout)
 results[-1]['restored_exit']=restored.returncode
 (out/'controls.json').write_text(json.dumps({'sha':sha,'stopped_after_control':len(results),'controls':results},indent=2)+'\n')
 print(name,'broken',run.returncode,'caught',caught,'restored',restored.returncode,flush=True)
 assert caught, f'SURVIVOR/failed harness: {name} at {file}; inspect broken log'
 assert restored.returncode==0,(name,restored.stdout)
print('Eight named assertion controls caught and restored; no survivors in this floor.',flush=True)
