"""One serial compiling native mutation per new private-order family, exact restoration."""
from pathlib import Path
import subprocess,os,json,argparse
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_private_orders.rs';saved=file.read_bytes();text=saved.decode();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();prefix='/tmp/woo-private-orders-'+sha[:10]+'-'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','private_orders','--jobs','2','--offline','--locked']
start=text.index('struct WooV3AlgoOrderFields {');end=text.index('\n}',start)+2;decl=text[start:end];swapped=decl.replace('rename = "reduceOnly"','rename = "TEMP"').replace('rename = "isActivated"','rename = "reduceOnly"').replace('rename = "TEMP"','rename = "isActivated"')
start=text.index('pub fn decode_regular_order');end=text.index('// ts: decodeAlgoQuantity',start);regular=text[start:end];assert regular.count('order_id: require_integer(row.get("orderId"), &format!("{path}.orderId"), 1.0)?,')==1
changed=regular.replace('order_id: require_integer(row.get("orderId"), &format!("{path}.orderId"), 1.0)?,','order_id: require_integer(row.get("orderId"), &format!("{path}.orderId"), 1.0)? + 1.0,')
controls=[
 ('declarations',decl,swapped,'native_declarations','PRIVATE-ORDERS native reduceOnly'),
 ('helpers','WooV3RegularOrderType::Ioc => (OrderType::Limit, Some(OrderTimeInForce::ImmediateOrCancel)),','WooV3RegularOrderType::Ioc => (OrderType::Limit, Some(OrderTimeInForce::FillOrKill)),','native_mapping_helpers','PRIVATE-ORDERS native timeInForce'),
 ('regular',regular,changed,'native_regular_orders','PRIVATE-ORDERS native orderId'),
 ('algo-decoding','if depth > 8 {','if depth >= 8 {','native_algo_decoding','PRIVATE-ORDERS native expected success'),
 ('algo-mapping','parent_id: Some(safe_cex_num::js_number_string(root_id)),','parent_id: Some(safe_cex_num::js_number_string(row.root_algo_order_id)),','native_algo_mapping','PRIVATE-ORDERS native parentId'),
 ('transactions','(row.executed_timestamp/1000.0).floor()','(row.executed_timestamp/1000.0).trunc()','native_transactions','PRIVATE-ORDERS native execution timestamp'),
]
parser=argparse.ArgumentParser();parser.add_argument('--start-at',choices=[c[0] for c in controls],default=controls[0][0]);args=parser.parse_args();selected=controls[next(i for i,c in enumerate(controls) if c[0]==args.start_at):];results=[];receipt={'sha':sha,'command_base':command,'start_at':args.start_at,'iterations':0,'survivors':0,'controls':results}
def record():Path(prefix+'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
for name,old,new,test,assertion in selected:
 assert text.count(old)==1 and old!=new,(name,old)
 row={'family':name,'old':old,'new':new,'test':test,'assertion':assertion,'overlay_on_sha':sha,'call_site':'safe-cex-venue-woo/src/v3_private_orders.rs:'+str(text[:text.index(old)].count('\n')+1),'command':command+[test,'--','--exact']};results.append(row)
 try:
  file.write_text(text.replace(old,new));run=subprocess.run(row['command'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['exit']=run.returncode;row['log']=prefix+name+'.log';Path(row['log']).write_text(run.stdout)
  valid=run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout
  if not valid:row['finding']='survived' if run.returncode==0 else 'no named native assertion failure';receipt['survivors']+=int(run.returncode==0);record()
  assert valid,run.stdout
  print(name+' named failure exit101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(row['command'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);row['restored_exit']=restored.returncode;row['restored_log']=prefix+name+'-restored.log';Path(row['restored_log']).write_text(restored.stdout);receipt['iterations']+=1;record();assert restored.returncode==0,restored.stdout
 print(name+' restored exit0',flush=True)
