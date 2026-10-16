"""One isolated compiled production fault per newly ported socket method."""
from pathlib import Path
import json,os,re,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/ws_dispatch.rs';logs=Path(os.environ['POLYMARKET_DISPATCH_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket')],text=True).strip(),'commit gate inputs first'
controls=[
 ('public.connectAndSubscribe','public_connect','connect_public','!host.disposed() && host.socket()?.is_none()','!host.disposed()','native dispatch trace:public.connectAndSubscribe'),
 ('private.connectAndSubscribe','private_connect','connect_private','if !host.truthy(&host.credentials()?)','if host.truthy(&host.credentials()?)','independent replay count:private.connectAndSubscribe'),
 ('public.onMessage','public_message','route','host.stats(topic)?;\n        let length = host.handler_length(&handlers)?;','let length = host.handler_length(&handlers)?;\n        host.stats(topic)?;','native dispatch trace:public.onMessage'),
 ('private.onMessage','private_message','message_private','host.refresh_positions("authentication")?;','host.refresh_positions("wrong")?;','native refresh trigger'),
 ('public.subscribeToAsset','asset_subscribe','subscribe_asset','if host.has_asset(token)?','if !host.has_asset(token)?','native dispatch trace:public.subscribeToAsset'),
 ('public.unsubscribeFromAsset','asset_unsubscribe','unsubscribe_asset','if !host.has_asset(token)?','if host.has_asset(token)?','native dispatch trace:public.unsubscribeFromAsset'),
 ('public.subscribeToOrderBook','book_subscribe','subscribe_book','host.subscribe_asset(token)?;','let _ = token;','native dispatch trace:public.subscribeToOrderBook'),
 ('public.handleLastTradePrice','last_trade_price','last_trade','let last = safe_cex_num::parse_float(&String::from_utf16_lossy(&price));','let last = safe_cex_num::parse_float(&String::from_utf16_lossy(&price)) + 1.0;','native Number bits'),
]
assert len(controls)==8 and controls[-1][0]=='public.handleLastTradePrice'
original=source.read_text();summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':8,'floor':'baseline only','controls':[]}

def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','ws_dispatch','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout);return r
baseline=run('baseline');assert baseline.returncode==0 and '9 passed; 0 failed' in baseline.stdout,baseline.stdout;summary['baseline_exit']=baseline.returncode
for name,test,function,old,new,assertion in controls:
 match=re.search(r'\n(?:pub )?fn '+re.escape(function)+r'\b',original);assert match,name;begin=match.start();following=re.search(r'\n(?:pub )?fn ',original[match.end():]);end=match.end()+following.start() if following else len(original);body=original[begin:end];assert body.count(old)==1,name
 try:
  source.write_text(original[:begin]+body.replace(old,new)+original[end:]);broken=run(name+'-mutation',test);caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'method':name,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} controls executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure: {name}\n{broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
