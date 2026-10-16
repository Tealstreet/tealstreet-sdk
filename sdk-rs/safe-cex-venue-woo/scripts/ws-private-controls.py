"""One compiling native control per private payload slice family; serial cargo, exact restoration."""
from pathlib import Path
import subprocess,os,json
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_ws_private.rs';saved=file.read_bytes();text=saved.decode()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','ws_private','--jobs','2','--offline','--locked']
def function_patch(name, old, new):
 start=text.index('pub fn '+name+'(')
 end=text.find('// ts:',start)
 if end<0:end=len(text)
 block=text[start:end]
 assert block.count(old)==1,(name,old)
 return block,block.replace(old,new)
start=text.index('struct WooV3WsBalanceRowFields {')
end=text.index('impl Serialize for WooV3WsBalanceRow',start)
block=text[start:end]
swapped=block.replace('rename = "holding"','rename = "TEMP"').replace('rename = "frozen"','rename = "holding"').replace('rename = "TEMP"','rename = "frozen"')
controls=[
 ('declaration-models',block,swapped,'WS-PRIVATE native WooV3WsBalanceRow holding'),
 ('account', 'let free_collateral = require_decimal(row.get("fc"), &format!("{path}.fc"))?;', 'let free_collateral = require_decimal(row.get("tc"), &format!("{path}.fc"))?;', 'WS-PRIVATE native WooV3WsAccount freeCollateral'),
 ('table-rows',*function_patch('decode_position_rows','pending_long_quantity: require_decimal(row.get("plq"), &format!("{path}.plq"))?,','pending_long_quantity: require_decimal(row.get("psq"), &format!("{path}.plq"))?,'),'WS-PRIVATE native WooV3WsPositionRow pendingLongQuantity'),
 ('execution',*function_patch('decode_execution_report','executed_price: require_decimal(row.get("epx"), &format!("{path}.epx"))?,','executed_price: require_decimal(row.get("px"), &format!("{path}.epx"))?,'),'WS-PRIVATE native WooV3WsExecutionReport executedPrice'),
 ('algo','trigger_price: require_decimal(row.get("trp"), &format!("{path}.trp"))?,','trigger_price: require_decimal(row.get("px"), &format!("{path}.trp"))?,','WS-PRIVATE native WooV3WsAlgoExecutionRow triggerPrice'),
 ('dispatch-and-frame','WooV3WsBlockerCode::PrivateBalanceReplacementSemanticsUndocumented,','WooV3WsBlockerCode::PrivateAccountReplacementSemanticsUndocumented,','WS-PRIVATE native blocker code'),
]
results=[]
for name,old,new,assertion in controls:
 try:
  assert text.count(old)==1 and old!=new,(name,old)
  file.write_text(text.replace(old,new))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-ws-private-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-ws-private-'+name+'-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,restored.stdout
 results.append({'family':name,'old':old,'new':new,'assertion':assertion,'exit':run.returncode,'restored_exit':restored.returncode,'call_site':'safe-cex-venue-woo/src/v3_ws_private.rs:'+str(text[:text.index(old)].count('\n')+1)})
 print(name+' restored exit 0',flush=True)
Path('/tmp/woo-ws-private-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'iterations':6,'survivors':0},indent=2)+'\n')
