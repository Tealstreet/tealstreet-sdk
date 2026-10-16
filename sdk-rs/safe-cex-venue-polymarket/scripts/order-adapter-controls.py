"""Eight isolated row faults and two reviewed async-body boundary faults."""
from pathlib import Path
import json,os,re,subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/order_adapters.rs';logs=Path(os.environ['POLYMARKET_ORDER_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket')],text=True).strip(),'commit inputs first'
original=source.read_text()
def span(function):
 match=re.search(r'\n(?:pub )?(?:async )?fn '+re.escape(function)+r'\b',original);assert match,function
 begin=match.start();brace=original.index('{',match.end());depth=1;end=brace+1
 while depth:
  if original[end]=='{':depth+=1
  if original[end]=='}':depth-=1
  end+=1
 return begin,end
begin,end=span('self_start');block=original[begin:end];owner_start=block.index('host.start_task(');owner_end=block.index('}));',owner_start)+4;owner_old=block[owner_start:owner_end]
controls=[
 ('exchange.mapOrder','exchange_map','map_exchange_order','mapped(host, data, false)','mapped(host, data, true)','native token map mode'),
 ('private.mapOrder','private_map','map_private_order','mapped(host, data, true)','mapped(host, data, false)','native token map mode'),
 ('exchange.fetchOrders','orders_read','fetch_orders','Some(value) => Some(host.map_order(&value)?),','Some(value) => { let _ = host.map_order(&value)?; None },','native mapped slot:fetchOrders'),
 ('exchange.cancelOrders','cancel_many','cancel_orders','host.field(&order, "id")?','host.field(&order, "asset_id")?','native cancellation id'),
 ('exchange.cancelAllOrders','cancel_all','cancel_all_orders','"info"','"debug"','native log severity'),
 ('exchange.fetchOrderBook','book_read','fetch_order_book','Ok(NativeBook { bids, asks })','Ok(NativeBook { bids: asks, asks: bids })','independent native book bid count'),
 ('exchange.listenOrderBook','book_listen','listen_order_book','host.connect()?;','let _ = host;','native synchronous prefix:exchange.listenOrderBook'),
 ('private.handleOrderUpdate','order_update','handle_order_update','amount: o.filled','amount: o.amount','native fill amount:native Number bits'),
 ('boundary.taskOwnership','abandoned_observers_keep_method_effects','self_start',owner_old,'let _ = (host, body, output);','native abandoned method effects:fetchOrders'),
 ('boundary.awaitYield','ready_child_operations_yield_before_effects','observe','yielded: false','yielded: true','native ready-child prefix:fetchOrders'),
]
assert len(controls)==10 and controls[-1][0]=='boundary.awaitYield'
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':10,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','order_adapters','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');assert baseline.returncode==0 and '10 passed; 0 failed' in baseline.stdout,baseline.stdout;summary['baseline_exit']=baseline.returncode
for name,test,function,old,new,assertion in controls:
 begin,end=span(function);block=original[begin:end];assert block.count(old)==1,name
 try:
  source.write_text(original[:begin]+block.replace(old,new)+original[end:]);broken=run(name+'-mutation',test);caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'method':name,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} controls executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure: {name}\n{broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
