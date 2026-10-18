"""Serial compiling native controls; save every measured result and restore exact bytes."""
from pathlib import Path
import subprocess,os,json,argparse
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_private.rs';saved=file.read_bytes();text=saved.decode();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();prefix='/tmp/woo-private-account-'+sha[:10]+'-'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','private_account','--jobs','1','--offline','--locked']
start=text.index('struct WooV3PageMetaFields {');end=text.index('\n}',start)+2;declaration=text[start:end];swapped=declaration.replace('rename = "total"','rename = "TEMP"').replace('rename = "currentPage"','rename = "total"').replace('rename = "TEMP"','rename = "currentPage"')
start=text.index('    let meta = decode_meta',text.index('pub fn decode_paginated_rows'));end=text.index('    if rows.len()',start);page=text[start:end];meta=page.splitlines(keepends=True)[0];assert meta.endswith('?;\n');reordered=page[len(meta):]+meta
controls=[
 ('declarations',declaration,swapped,'native_declarations','PRIVATE-ACCOUNT native total'),
 ('validators','let number = value.and_then(|v| v.clone().decode::<f64>().ok());','let number = value.and_then(|v| v.clone().decode::<f64>().ok()).map(f64::abs);','native_helpers_and_numeric_mapping','PRIVATE-ACCOUNT native number bits'),
 ('assets','&row.available_balance,','&row.frozen,','native_asset_reads_and_mapping','PRIVATE-ACCOUNT native tradingFree'),
 ('futures','} else if signed_holding > 0.0 {','} else if signed_holding < 0.0 {','native_futures_reads_and_mapping','PRIVATE-ACCOUNT native position side'),
 ('pagination',page,reordered,'native_envelopes_metadata_and_callback_order','PRIVATE-ACCOUNT native metadata-before-callback order'),
]
parser=argparse.ArgumentParser();parser.add_argument('--start-at',choices=[c[0] for c in controls],default=controls[0][0]);args=parser.parse_args();selected=controls[next(i for i,c in enumerate(controls) if c[0]==args.start_at):];results=[]
receipt={'sha':sha,'start_at':args.start_at,'command_base':command,'controls':results,'iterations':0,'survivors':0}
def record():Path(prefix+'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
for name,old,new,test,assertion in selected:
 assert text.count(old)==1 and old!=new,(name,old)
 row={'family':name,'old':old,'new':new,'test':test,'assertion':assertion,'overlay_on_sha':sha,'call_site':'safe-cex-venue-woo/src/v3_private.rs:'+str(text[:text.index(old)].count('\n')+1),'command':command+[test,'--','--exact']};results.append(row)
 try:
  file.write_text(text.replace(old,new));run=subprocess.run(row['command'],cwd=crate.parent,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['exit']=run.returncode;row['log']=prefix+name+'.log';Path(row['log']).write_text(run.stdout)
  valid=run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout
  if not valid:
   row['finding']='survived' if run.returncode==0 else 'no named native assertion failure';receipt['survivors']+=int(run.returncode==0);record()
  assert valid,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(row['command'],cwd=crate.parent,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['restored_exit']=restored.returncode;row['restored_log']=prefix+name+'-restored.log';Path(row['restored_log']).write_text(restored.stdout);receipt['iterations']+=1;record();assert restored.returncode==0,restored.stdout
 print(name+' restored exit 0',flush=True)
