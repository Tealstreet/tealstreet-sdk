"""Recovered-private-member cost campaign. Serial Cargo; finite selected controls."""
from pathlib import Path
import argparse,hashlib,importlib.util,json,subprocess,statistics
spec=importlib.util.spec_from_file_location('base_driver',Path(__file__).with_name('class_cost.py'))
d=importlib.util.module_from_spec(spec);spec.loader.exec_module(d)
EVID=d.EVID/'resolution-cost'; EVID.mkdir(exist_ok=True); d.EVID=EVID
original_inputs=d.input_hashes
EXTRA=['sdk-rs/safe-cex-venue-polymarket/tests/'+f for f in ['resolution_cost.py','resolution_cost.rs','resolution_cost.cjs','order_adapters.rs','order_adapters_oracle.cjs']]+['packages/safe-cex/src/types.ts','packages/logging/src/LogSeverity.ts','sdk-rs/parity/evidence/polymarket-oracle-classes/inventory.json','sdk-rs/parity/evidence/polymarket-oracle-classes/census.json','sdk-rs/parity/evidence/polymarket-oracle-classes/resolution-cost/protocol.json','sdk-rs/parity/evidence/polymarket-oracle-classes/resolution-cost/profile-expansion.json']
def inputs():return {**original_inputs(),**{f:hashlib.sha256((d.ROOT/f).read_bytes()).hexdigest() for f in EXTRA}}
d.input_hashes=inputs
SAMPLES=[('exchange','resolution_exchange_map_native','PolymarketExchange#mapOrder'),('private','resolution_private_map_native','PolymarketPrivateWebSocket#mapOrder'),('update','resolution_update_native','PolymarketPrivateWebSocket#handleOrderUpdate')]
MUTATIONS=[
 ('exchange-amount','order_adapters.rs','let amount = number(host, &host.field(data, "original_size")?)?;','let amount = number(host, &host.field(data, "original_size")?)? + 1.0;', 'resolution_exchange_map_native','resolution_native_amount'),
 ('private-sell', 'order_adapters.rs', '    mapped(host, data, true)\n}', '    let mut order = mapped(host, data, true)?;\n    if host.is_text(&order.side, "sell") { order.side = host.literal("buy"); }\n    Ok(order)\n}', 'resolution_private_map_native', 'resolution_native_side:private:16'),
 ('update-fill','order_adapters.rs','amount: o.filled,','amount: o.amount,','resolution_update_native','resolution_native_fill_amount'),
 ('id-length65','order_adapters.rs','    mapped(host, data, false)\n}', '    let mut order = mapped(host, data, false)?;\n    if host.string(&order.id)?.len() == 65 { order.id = host.literal("fault"); }\n    Ok(order)\n}', 'resolution_exchange_map_native','resolution_native_id:exchange:192'),
 ('source-price-erasure','../tests/order_adapters_oracle.cjs','keys.map(k=>[k,value(o[k])])','keys.map(k=>[k,value(k===\'price\'?o.amount:o[k])])','resolution_private_map_native','native mapped Order 5'),
 ('private-map-required','order_adapters.rs','mapped(host, data, true)','mapped(host, data, false)','resolution_private_map_native','native token map mode'),
]
def cargo(test):return ['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-polymarket','--locked','--jobs','2','--test','resolution_cost','--',test,'--exact','--nocapture']
def positive(label,test,metadata=None):
 r,log=d.run(label,cargo(test),{'test':test,**(metadata or {})})
 assert r['exit']==0 and f'test {test} ... ok' in log and 'test result: ok. 1 passed; 0 failed;' in log
 return r
def baseline():
 for family,test,row in SAMPLES:positive('baseline-'+family,test,{'row':row,'cases':512})
def runtime():
 for n in range(3):
  for family,test,row in SAMPLES:positive(f'warm-{n}-{family}',test,{'row':row,'repetition':n,'cases':512})
def controls():
 outcomes=[]
 for name,file,before,after,test,assertion in MUTATIONS:
  path=(d.SRC/file).resolve(); relative=str(path.relative_to(d.ROOT)); original=path.read_bytes();source=original.decode()
  assert original==subprocess.check_output(['git','show',d.sha()+':'+relative],cwd=d.ROOT)
  assert source.count(before)==1,(name,source.count(before))
  try:
   path.write_text(source.replace(before,after))
   r,log=d.run('mutant-'+name,cargo(test),{'test':test,'mutation':name,'source':relative,'original_sha256':hashlib.sha256(original).hexdigest(),'mutated_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'expected_assertion':assertion})
  finally:path.write_bytes(original)
  restore,restore_log=d.run('restore-'+name,cargo(test),{'test':test,'restored_sha256':hashlib.sha256(original).hexdigest()})
  caught=r['exit']==101 and assertion in log and 'panicked at' in log and f'test {test} ... FAILED' in log and 'test result: FAILED. 0 passed; 1 failed;' in log and 'could not compile' not in log
  restored=restore['exit']==0 and f'test {test} ... ok' in restore_log and 'test result: ok. 1 passed; 0 failed;' in restore_log
  outcomes.append({'name':name,'sha':r['sha'],'test':test,'assertion':assertion,'mutant_exit':r['exit'],'restore_exit':restore['exit'],'caught':caught,'restored':restored})
  (EVID/'controls.json').write_text(json.dumps({'sha':d.sha(),'expected_controls':6,'stop_index':5,'outcomes':outcomes,'survivors':[r for r in outcomes if r['mutant_exit']==0],'validation_failures':[r for r in outcomes if not r['caught'] or not r['restored']]},indent=2)+'\n')
 assert len(outcomes)==6 and all(r['caught'] and r['restored'] for r in outcomes),outcomes
 print('CONTROL FLOOR:5 Rust+1source caught;6restorations;0selectedsurvivors;stop5',flush=True)
def census():
 old=json.loads((EVID.parent/'census.json').read_text()); rows=json.loads((EVID.parent/'inventory.json').read_text())
 assert len(rows)==7341 and old['counts']['unpriced_resolution']==1400
 r,log=d.run('resolution-census',['node','sdk-rs/safe-cex-venue-polymarket/tests/class_cost_census.cjs','--resolve-private'],stdin=json.dumps(rows))
 assert r['exit']==0
 current=json.loads(log); assert len(current['inventory'])==7341
 selected=[]
 for before,after in zip(old['inventory'],current['inventory'],strict=True):
  assert all(before[k]==after[k] for k in ['source','ts','kind','manifest'])
  if before['class']=='unpriced_resolution':selected.append(after)
 assert len(selected)==1400
 from collections import Counter
 counts=dict(Counter(row['class'] for row in selected)); assert sum(counts.values())==1400
 for family,test,symbol in SAMPLES:
  matches=[r for r in selected if r['ts']==symbol]; assert len(matches)==1 and matches[0]['class']=='receiver_sync',symbol
 (EVID/'resolved-census.json').write_text(json.dumps({'sha':d.sha(),'frozen_denominator':7341,'historical_resolution_population':1400,'resolved':1400-counts.get('unpriced_resolution',0),'counts':counts,'samples':[r for r in selected if r['ts'] in [s[2] for s in SAMPLES]],'inventory':selected},indent=2)+'\n')
 print('RECOVERED ROUTES',counts,flush=True)
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['census','baseline','controls','runtime']);args=parser.parse_args();globals()[args.mode]()
