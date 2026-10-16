"""One isolated compiled fault per newly ported REST method; restore in finally."""
from pathlib import Path
import json,os,re,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/price_reads.rs';logs=Path(os.environ['POLYMARKET_PRICE_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket')],text=True).strip(),'commit gate inputs first'
controls=[
 ('getMidpoint','midpoint','get_midpoint','number(&response.data.mid)','number(&response.data.price)','native price result:getMidpoint:native Number bits'),
 ('getMidpoints','midpoints','get_midpoints','vec![("token_id", id.clone())]','vec![("token_ids", id.clone())]','native mapped row key:getMidpoints'),
 ('getPrice','price_value','get_price','("side", ParamValue::String(side.to_vec()))','("market", ParamValue::String(side.to_vec()))','native parameter key:getPrice'),
 ('getLastTradePrice','last_trade_price','get_last_trade_price','number(&response.data.price)','{ let _ = response.data.price; 0.0 }','native price result:getLastTradePrice:native Number bits'),
 ('getPriceHistory','price_history','get_price_history','if truthy(&history)','if !truthy(&history)','native original array identity:getPriceHistory'),
 ('getOrder','order','get_order','url.extend_from_slice(order_id);','url.extend_from_slice(&[]); let _ = order_id;','native request URL:getOrder'),
 ('getBalanceAllowance','balance_allowance','get_balance_allowance','ParamValue::Number(signature_type)','ParamValue::Number(signature_type + 1.0)','native numeric parameter:getBalanceAllowance:native Number bits'),
 ('deriveApiKey','derive_key','derive_api_key','("POLY_ADDRESS", address.to_vec())','("POLY_ADDRESS", { let _ = address; signature.to_vec() })','native header units:deriveApiKey'),
]
assert len(controls)==8 and controls[-1][0]=='deriveApiKey'
original=source.read_text();summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'floor':'eight new method bodies; stopped after deriveApiKey','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','price_reads','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout);return r
baseline=run('baseline');assert baseline.returncode==0 and '10 passed; 0 failed' in baseline.stdout,baseline.stdout;summary['baseline_exit']=baseline.returncode
for name,test,function,old,new,assertion in controls:
 match=re.search(r'\npub fn '+re.escape(function)+r'\b',original);assert match,name;begin=match.start();following=re.search(r'\npub fn ',original[match.end():]);end=match.end()+following.start() if following else len(original);body=original[begin:end];assert body.count(old)==1,(name,body.count(old))
 try:
  source.write_text(original[:begin]+body.replace(old,new)+original[end:]);broken=run(name+'-mutation',test);caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'method':name,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure: {name}\n{broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
