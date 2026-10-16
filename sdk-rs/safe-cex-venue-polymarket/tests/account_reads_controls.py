"""Eight body faults and ten semantic boundary/source faults; serial compiled controls."""
from pathlib import Path
import json, os, re, subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/account_reads.rs'
logs=Path(os.environ['POLYMARKET_ACCOUNT_READ_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket'),str(crate.parent.parent/'packages/safe-cex/src/exchanges/polymarket')],text=True).strip(),'commit all inputs before controls'
original=source.read_text()
def span(name):
 match=re.search(r'\bfn '+re.escape(name)+r'[<(]',original);assert match,name
 start=match.start();begin=original.index('{',start);end=begin+1;depth=1
 while depth:
  if original[end]=='{':depth+=1
  if original[end]=='}':depth-=1
  end+=1
 return start,end
controls=[
 ('postOrder','post_order_native','post_order','host.field(&credentials, "apiKey")?','host.field(&credentials, "secret")?','native text postOrder owner'),
 ('postOrders','post_orders_native','post_orders','host.field(&credentials, "apiKey")?','host.field(&credentials, "secret")?','native text postOrders owner'),
 ('getOpenOrders','open_orders_native','get_open_orders','host.text("MA==")','host.text("bad cursor")','native text cursor native value'),
 ('getTrades','trades_native','get_trades','host.slice(&trades, &limit)?','trades.clone()','native pagination count getTrades/4'),
 ('getGammaMarkets','gamma_markets_native','get_gamma_markets','offset = host.add(&offset, &limit)?;','offset = offset.clone();','native Number bits gamma offset'),
 ('getPositions','positions_native','get_positions','("sizeThreshold", host.number(0.0))','("sizeThreshold", host.number(1.0))','native Number bits positions threshold'),
 ('getAccount','account_native','get_account','("affiliateId", host.null())','("affiliateId", host.undefined())','native account affiliateId null'),
 ('validateAccount','validation_native','validate_account','host.text("Failed to validate account")','host.text("Wrong fallback")','native text validation error field'),
 ('boundary.Ownership','observer_retains_whole_method','start',None,None,'native dropped observer completes all three pages'),
 ('boundary.ReadyAwait','ready_await_yields','wait','yielded: false','yielded: true','native ready await defers page two'),
 ('boundary.ImmediateReturn','immediate_return_has_no_await','returned','yielded: true','yielded: false','native non-thenable return has no await turn'),
 ('boundary.ReturnResolution','return_resolution_is_awaited','returned',None,None,'native return resolver invoked'),
 ('boundary.DirectArrayAlias','open_orders_native','get_open_orders','return returned(host.as_ref(), data).await;','return returned(host.as_ref(), host.slice(&data, &host.number(host.length(&data)? as f64))?).await;','native direct page identity getOpenOrders/0'),
 ('boundary.InheritedSetters','open_orders_native','get_open_orders',None,None,'native inherited setter count'),
 ('boundary.ArrayMapFloor','post_orders_native','post_orders','host.map_output(&orders, length)?','host.map_output(&orders, length + 1)?','native fixed Array.map length'),
 ('boundary.LoggerCapture','validation_native','validate_account',None,None,'native captured logger identity'),
 ('boundary.GammaFlags','gamma_markets_native','get_gamma_markets','host.spread_params(&params, limit.clone(), offset.clone())?','host.spread_params(&host.undefined(), limit.clone(), offset.clone())?','native independent own-key count gamma params'),
 ('boundary.NestedOrderAlias','post_orders_native','post_orders','("order", host.field(&o, "order")?)','("order", host.object(vec![("marker", host.number(71.0))])?)','native nested order original pointer alias'),
]
assert len(controls)==18 and controls[-1][0]=='boundary.NestedOrderAlias'
start_at=int(os.environ.get('POLYMARKET_ACCOUNT_READ_CONTROL_FROM','1'));assert 1<=start_at<=18
summary={'start_index':start_at,'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':18,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','account_reads','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');assert baseline.returncode==0 and '12 passed; 0 failed' in baseline.stdout,baseline.stdout
for index,(name,test,owner,old,new,assertion) in enumerate(controls,1):
 if index<start_at:continue
 begin,end=span(owner);block=original[begin:end]
 if name=='boundary.Ownership':
  assert block.count('host.start_task(Box::pin(async move {')==1 and block.count('    }));\n    Observer(state)')==1
  mutated=block.replace('host.start_task(Box::pin(async move {','let _ = host; let mut task = Box::pin(async move {').replace('    }));\n    Observer(state)','    }); let mut cx=Context::from_waker(Waker::noop()); let _ = task.as_mut().poll(&mut cx);\n    Observer(state)')
 elif name=='boundary.ReturnResolution':mutated=block[:block.index('{')+1]+' let _ = host; Ok(value) }'
 elif name=='boundary.LoggerCapture':
  old='                    let logger = host.capture("exchange", "log")?;\n';assert block.count(old)==1
  mutated=block.replace(old,'').replace('                    host.invoke_log(logger, &log, "error")?;',old+'                    host.invoke_log(logger, &log, "error")?;')
 elif name=='boundary.InheritedSetters':
  old='''let params = host.object(vec![("next_cursor", cursor)])?;
                if host.truthy(&market) {
                    host.assign(&params, "market", market.clone())?;
                }'''
  assert block.count(old)==1;mutated=block.replace(old,'''let mut fields = vec![("next_cursor",cursor)]; if host.truthy(&market){ fields.push(("market",market.clone())); } let params=host.object(fields)?;''')
 else:assert block.count(old)==1,(name,old);mutated=block.replace(old,new)
 try:
  source.write_text(original[:begin]+mutated+original[end:]);broken=run(name+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'family':name,'target':'src/account_reads.rs::'+owner,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {owner}: {broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
