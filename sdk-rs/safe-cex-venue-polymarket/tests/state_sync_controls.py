"""One compiled discriminating control per newly ported state-sync row."""
from pathlib import Path
import json, os, re, subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/state_sync.rs'
logs=Path(os.environ['POLYMARKET_STATE_SYNC_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
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
controls=[('deriveApiCredentials', 'derive_native', 'derive_api_credentials', 'host.assign_credentials(credentials)?;', 'let _ = credentials;', 'native derived credentials original alias'), ('start', 'start_native', 'start_exchange', 'Some(("markets", markets.clone()))', 'Some(("markets", host.array(vec![])))', 'native startup original markets alias'), ('tick', 'tick_native', 'tick', 'host.sum_by(&positions, "unrealizedPnl")?', 'host.number(0.0)', 'native bits tick.upnl'), ('fetchBalance', 'balance_native', 'fetch_balance', '/ 1e6', '/ 1e3', 'native bits balance.total'), ('fetchPositions', 'positions_native', 'fetch_positions', '("mergeable", host.field(&holding, "mergeable")?)', '("mergeable", host.field(&holding, "redeemable")?)', 'native mergeable'), ('fetchAndSetPositions', 'snapshot_native', 'fetch_and_set_positions', 'Some(("positions", positions.clone()))', 'Some(("positions", host.array(vec![])))', 'native snapshot committed original alias'), ('refreshPositions', 'refresh_native', 'refresh_positions', '"Error refreshing positions after "', '"Wrong "', 'native refresh error text'), ('handleTradeUpdate', 'trade_native', 'handle_trade_update', 'let amount = safe_cex_num::parse_float(&String::from_utf16_lossy(\n            &host.string(&host.field(&data, "size")?)?,\n        ));', 'let amount = safe_cex_num::parse_float(&String::from_utf16_lossy(\n            &host.string(&host.field(&data, "price")?)?,\n        ));', 'native bits fill.amount')]
assert len(controls)==8 and controls[-1][0]=="handleTradeUpdate"
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':8,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','state_sync','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');assert baseline.returncode==0 and '8 passed; 0 failed' in baseline.stdout,baseline.stdout
for index,(name,test,owner,old,new,assertion) in enumerate(controls,1):
 begin,end=span(owner);block=original[begin:end]
 assert block.count(old)==1,(name,old);mutated=block.replace(old,new)
 try:
  source.write_text(original[:begin]+mutated+original[end:]);broken=run(name+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'family':name,'target':'src/state_sync.rs::'+owner,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {owner}: {broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
