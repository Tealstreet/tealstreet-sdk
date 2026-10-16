"""Six serial native family controls; no concurrent Cargo; restore original bytes after each mutation."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_regular_writes.rs';saved=file.read_bytes();text=saved.decode();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();prefix='/tmp/woo-regular-'+sha[:10]+'-'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','regular_writes','--jobs','2','--offline','--locked']
start=text.index('struct WooV3RegularPlaceInputFields {');end=text.index('\n}',start)+2;decl=text[start:end];swapped=decl.replace('rename = "price"','rename = "TEMP"').replace('rename = "quantity"','rename = "price"').replace('rename = "TEMP"','rename = "quantity"')
start=text.index('pub fn build_woo_v3_regular_cancel_pending_request');end=text.index('// Supporting anonymous decodeSuccessData result.',start);cancel=text[start:end];changed_cancel=cancel.replace('WooV3Endpoint::TradeOrders','WooV3Endpoint::TradeOrder')
controls=[
 ('declarations',decl,swapped,'native_declarations','REGULAR-WRITES native declaration quantity'),
 ('validators','cmp_utf16(&l, &r)','-cmp_utf16(&l, &r)','native_validators','REGULAR-WRITES native decimal comparison'),
 ('place','let pos = if reduce {','let pos = if !reduce {','native_place_builders','REGULAR-WRITES native captured string'),
 ('amend','if !matches {','if matches {','native_amend_builders','REGULAR-WRITES native blocked variant'),
 ('cancel',cancel,changed_cancel,'native_cancel_builders','REGULAR-WRITES native request endpoint'),
 ('responses','name: "WooV3WriteResponseError".into(),','name: "WrongWriteResponseError".into(),','native_response_decoders','REGULAR-WRITES native gateway name'),
]
results=[];receipt={'sha':sha,'command_base':command,'iterations':0,'survivors':0,'controls':results}
def record():Path(prefix+'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
for name,old,new,test,assertion in controls:
 assert text.count(old)==1 and old!=new,(name,old)
 row={'family':name,'old':old,'new':new,'test':test,'assertion':assertion,'overlay_on_sha':sha,'call_site':'safe-cex-venue-woo/src/v3_regular_writes.rs:'+str(text[:text.index(old)].count('\n')+1),'command':command+[test,'--','--exact']};results.append(row)
 try:
  file.write_text(text.replace(old,new));run=subprocess.run(row['command'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['exit']=run.returncode;row['log']=prefix+name+'.log';Path(row['log']).write_text(run.stdout)
  valid=run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout
  if not valid:row['finding']='survived' if run.returncode==0 else 'no named native assertion failure';receipt['survivors']+=int(run.returncode==0)
 finally:file.write_bytes(saved)
 record();restored=subprocess.run(row['command'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['restored_exit']=restored.returncode;row['restored_log']=prefix+name+'-restored.log';Path(row['restored_log']).write_text(restored.stdout);receipt['iterations']+=1;record()
 assert restored.returncode==0,restored.stdout
 assert valid,run.stdout
 print(name+' named failure exit101: '+assertion+'; restored exit0',flush=True)
