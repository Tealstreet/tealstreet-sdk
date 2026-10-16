"""Eight record families, three native boundary faults, three declaration drifts and distinct timestamp identities."""
from pathlib import Path
import os, json, re, subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/ws_records.rs';typescript=crate.parent.parent/'packages/safe-cex/src/exchanges/polymarket/polymarket.types.ts'
logs=Path(os.environ['POLYMARKET_WS_RECORD_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket'),str(typescript)],text=True).strip(),'commit all inputs before controls'
original=source.read_text();ts_original=typescript.read_text()
def span(name):
 match=re.search(r'\nimpl Decode for '+re.escape(name)+r' \{',original);assert match,name
 start=match.start();end=match.end();depth=1
 while depth:
  if original[end]=='{':depth+=1
  if original[end]=='}':depth-=1
  end+=1
 return start,end
controls=[
 ('PolymarketAPIOrder','apiorder_native','PolymarketAPIOrder','id: required(fields, "id")?','id: required(fields, "owner")?','native text PolymarketAPIOrder.id/0'),
 ('PolymarketTrade','trade_native','PolymarketTrade','taker_order_id: required(fields, "taker_order_id")?','taker_order_id: required(fields, "maker_order_id")?','native text PolymarketTrade.taker_order_id/0'),
 ('PolymarketWSSubscription','wssubscription_native','PolymarketWSSubscription','markets: optional(fields, "markets")?','markets: optional(fields, "assets_ids")?','native text PolymarketWSSubscription.markets[0]/1'),
 ('PolymarketWSOperation','wsoperation_native','PolymarketWSOperation','assets_ids: optional(fields, "assets_ids")?','assets_ids: optional(fields, "markets")?','native text PolymarketWSOperation.assets_ids[0]/1'),
 ('PolymarketWSPriceChange','wspricechange_native','PolymarketWSPriceChange','price_changes: required(fields, "price_changes")?','price_changes: vec![]','native array count PolymarketWSPriceChange.price_changes/0'),
 ('PolymarketWSOrderBookUpdate','wsorderbookupdate_native','PolymarketWSOrderBookUpdate','bids: optional(fields, "bids")?','bids: optional(fields, "asks")?','native text PolymarketWSOrderBookUpdate.bids[0].price/1'),
 ('PolymarketWSTrade','wstrade_native','PolymarketWSTrade','trade: PolymarketTrade::decode(value)?','trade: { let mut trade = PolymarketTrade::decode(value)?; std::mem::swap(&mut trade.taker_order_id, &mut trade.maker_order_id); trade }','native text PolymarketWSTrade.taker_order_id/0'),
 ('PolymarketWSOrderUpdate','wsorderupdate_native','PolymarketWSOrderUpdate','order: PolymarketAPIOrder::decode(value)?','order: { let mut order = PolymarketAPIOrder::decode(value)?; std::mem::swap(&mut order.id, &mut order.owner); order }','native text PolymarketWSOrderUpdate.id/0'),
 ('boundary.UTF16','trade_native','Text','Ok(Text(v.clone()))','Ok(Text(String::from_utf16_lossy(v).encode_utf16().collect()))','native text PolymarketTrade.id/1'),
 ('boundary.NumberSign','trade_native','f64','NativeValue::Number(v) => Ok(*v)','NativeValue::Number(v) => Ok(if *v == 0.0 { 0.0 } else { *v })','native Number bits PolymarketTrade.match_time'),
 ('boundary.OptionalPresence','wsoperation_native',None,'Some((_, NativeValue::Undefined)) => Ok(Optional::Undefined)','Some((_, NativeValue::Undefined)) => Ok(Optional::Absent)','native optional presence PolymarketWSOperation.assets_ids/3'),
 ('source.Intersection','wstrade_native','typescript',"export type PolymarketWSTrade = PolymarketTrade & { event_type: 'trade' };",'export type PolymarketWSTrade = PolymarketTrade;','actual eight source declarations including intersections/nested fields'),
 ('boundary.TimestampIdentity', 'apiorder_native', 'PolymarketAPIOrder', 'created_at: required(fields, "created_at")?', 'created_at: required(fields, "expiration")?', 'native Number bits PolymarketAPIOrder.created_at'),
 ('source.RequiredUndefined', 'trade_native', 'typescript', 'export interface PolymarketTrade {\n  /** Trade ID */\n  id: string;', 'export interface PolymarketTrade {\n  /** Trade ID */\n  id: string | undefined;', 'required undefined union refused'),
 ('source.ArrayUndefined', 'wsoperation_native', 'typescript', 'export interface PolymarketWSOperation {\n  /** Asset IDs */\n  assets_ids?: string[];', 'export interface PolymarketWSOperation {\n  /** Asset IDs */\n  assets_ids?: (string | undefined)[];', 'required undefined union refused'),
]
assert len(controls)==15 and controls[-1][0]=='source.ArrayUndefined'
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':15,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','ws_records','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');assert baseline.returncode==0 and '10 passed; 0 failed' in baseline.stdout,baseline.stdout
for name,test,owner,old,new,assertion in controls:
 target=typescript if owner=='typescript' else source;content=ts_original if owner=='typescript' else original
 begin,end=(0,len(content)) if owner in [None,'typescript'] else span(owner)
 block=content[begin:end];assert block.count(old)==1,name
 try:
  target.write_text(content[:begin]+block.replace(old,new)+content[end:]);broken=run(name+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:target.write_text(content)
 restored=run(name+'-restored',test);summary['controls'].append({'family':name,'target':str(target),'owner':owner,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {owner}: {broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original and typescript.read_text()==ts_original
