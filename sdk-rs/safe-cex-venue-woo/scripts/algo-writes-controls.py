"""Six exact native controls on one combined overlay; serial Cargo; restore original bytes before six clean reruns."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_algo_writes.rs';saved=file.read_bytes();text=saved.decode();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();prefix='/tmp/woo-algo-'+sha[:10]+'-'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','algo_writes','--jobs','2','--offline','--locked']
start=text.index('struct WooV3StopMarketPlaceInputFields {');end=text.index('\n}',start)+2;decl=text[start:end];swapped=decl.replace('rename = "quantity"','rename = "TEMP"').replace('rename = "triggerPrice"','rename = "quantity"').replace('rename = "TEMP"','rename = "triggerPrice"')
start=text.index('pub fn require_array');end=text.index('// ts: requireBoolean',start);array=text[start:end];changed_array=array.replace('v.clone().decode().ok()','v.clone().decode::<Vec<JsNumberInput>>().ok().map(|mut xs| { if !xs.is_empty(){xs.remove(0);} xs })')
start=text.index('    if compare_decimals(',text.index('pub fn build_woo_v3_algo_amend_request'));middle=text.index('    // Source declared isTriggered',start);end=text.index('    let update =',middle);ordered=text[start:end];reordered=text[middle:end]+text[start:middle]
start=text.index('pub fn build_woo_v3_algo_cancel_pending_request');end=text.index('// ts: decodeMutation',start);cancel=text[start:end];changed_cancel=cancel.replace('WooV3Endpoint::TradeAlgoOrders','WooV3Endpoint::TradeAlgoOrder')
controls=[
 ('declarations',decl,swapped,'native_declarations','ALGO-WRITES native declaration quantity'),
 ('helpers',array,changed_array,'native_validators','ALGO-WRITES native helper array length'),
 ('place','} else if side == "BUY" {\n        "SHORT"','} else if side == "BUY" {\n        "LONG"','native_place_builders','ALGO-WRITES native captured string'),
 ('amend-order',ordered,reordered,'native_amend_builders','ALGO-WRITES native first amend blocker'),
 ('cancel',cancel,changed_cancel,'native_cancel_builders','ALGO-WRITES native request endpoint'),
 ('responses','name: "WooV3AlgoWriteResponseError".into(),','name: "WrongAlgoWriteResponseError".into(),','native_response_decoders','ALGO-WRITES native gateway name'),
]
results=[];receipt={'sha':sha,'mode':'six simultaneous substitutions; six separate exact-test runs; one byte restoration; six clean exact-test reruns','command_base':command,'iterations':0,'survivors':0,'controls':results}
def record():Path(prefix+'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
overlay=text
for name,old,new,test,assertion in controls:
 assert text.count(old)==1 and old!=new,(name,old)
 overlay=overlay.replace(old,new)
 results.append({'family':name,'old':old,'new':new,'test':test,'assertion':assertion,'overlay_on_sha':sha,'call_site':'safe-cex-venue-woo/src/v3_algo_writes.rs:'+str(text[:text.index(old)].count('\n')+1),'command':command+[test,'--','--exact']})
assert overlay!=text
record()
try:
 file.write_text(overlay)
 for row in results:
  run=subprocess.run(row['command'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['exit']=run.returncode;row['log']=prefix+row['family']+'.log';Path(row['log']).write_text(run.stdout)
  row['named_failure']=run.returncode==101 and 'panicked at' in run.stdout and row['assertion'] in run.stdout and '0 passed; 1 failed;' in run.stdout
  if not row['named_failure']:row['finding']='survived' if run.returncode==0 else 'no named native assertion failure';receipt['survivors']+=int(run.returncode==0)
  record();print(row['family']+' combined-overlay exit '+str(run.returncode)+' named='+str(row['named_failure']),flush=True)
finally:file.write_bytes(saved)
assert file.read_bytes()==saved
for row in results:
 restored=subprocess.run(row['command'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['restored_exit']=restored.returncode;row['restored_log']=prefix+row['family']+'-restored.log';Path(row['restored_log']).write_text(restored.stdout);receipt['iterations']+=1;record()
 assert restored.returncode==0 and '1 passed; 0 failed;' in restored.stdout,restored.stdout
 print(row['family']+' restored exact test exit0',flush=True)
assert all(row['named_failure'] for row in results),receipt
