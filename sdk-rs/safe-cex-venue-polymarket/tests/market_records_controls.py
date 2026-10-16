"""Nine record families, four native boundary faults and two actual declaration drifts."""
from pathlib import Path
import os, json, re, subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/market_records.rs';typescript=crate.parent.parent/'packages/safe-cex/src/exchanges/polymarket/polymarket.types.ts';constants=typescript.with_name('polymarket.constants.ts')
logs=Path(os.environ['POLYMARKET_MARKET_RECORD_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket'),str(typescript),str(constants)],text=True).strip(),'commit all inputs before controls'
original=source.read_text();ts_original=typescript.read_text();constants_original=constants.read_text()
def span(name):
 match=re.search(r'\nimpl Decode for '+re.escape(name)+r' \{',original);assert match,name
 start=match.start();end=match.end();depth=1
 while depth:
  if original[end]=='{':depth+=1
  if original[end]=='}':depth-=1
  end+=1
 return start,end
controls=[
 ('PolymarketToken', 'token_native', 'Outcome', 'return Ok(Self::Yes);', 'return Ok(Self::No);', 'native literal PolymarketToken.outcome/0'),
 ('PolymarketGammaMarket', 'gamma_market_native', 'PolymarketGammaMarket', 'tokens: required(fields, "tokens")?', 'tokens: vec![]', 'native array count PolymarketGammaMarket.tokens/0'),
 ('PolymarketPosition', 'position_native', 'PolymarketPosition', 'token_id: required(fields, "token_id")?', 'token_id: required(fields, "condition_id")?', 'native text PolymarketPosition.token_id/0'),
 ('PolymarketSignedOrder', 'signed_order_native', 'PolymarketSignedOrder', 'maker_amount: required(fields, "makerAmount")?', 'maker_amount: required(fields, "takerAmount")?', 'native text PolymarketSignedOrder.makerAmount/0'),
 ('PolymarketCreateOrderRequest', 'create_order_request_native', 'PolymarketCreateOrderRequest', 'order: required(fields, "order")?', 'order: { let mut order: PolymarketSignedOrder = required(fields, "order")?; std::mem::swap(&mut order.salt, &mut order.maker); order }', 'native text PolymarketCreateOrderRequest.order.salt/0'),
 ('PolymarketCancelRequest', 'cancel_request_native', 'PolymarketCancelRequest', 'order_ids: optional(fields, "orderIds")?', 'order_ids: Optional::Absent', 'native optional presence PolymarketCancelRequest.orderIds/1'),
 ('PolymarketExchangeOptions', 'exchange_options_native', 'PolymarketExchangeOptions', 'api_key: optional(fields, "apiKey")?', 'api_key: optional(fields, "apiSecret")?', 'native text PolymarketExchangeOptions.apiKey/1'),
 ('GammaMarketInfo', 'gamma_market_info_native', 'GammaMarketInfo', 'id: required(fields, "id")?', 'id: required(fields, "slug")?', 'native text GammaMarketInfo.id/0'),
 ('PolymarketDataPosition', 'data_position_native', 'PolymarketDataPosition', 'size: required(fields, "size")?', 'size: required(fields, "avgPrice")?', 'native Number bits PolymarketDataPosition.size'),
 ('boundary.NumberOrText', 'gamma_market_info_native', 'NumberOrText', 'NativeValue::Number(v) => Ok(Self::Number(*v))', 'NativeValue::Number(v) => { let _ = v; Ok(Self::Text(Text(vec![48]))) }', 'native number/string arm GammaMarketInfo.orderPriceMinTickSize/0'),
 ('boundary.Nullable', 'gamma_market_info_native', 'GammaMarketInfo', 'order_min_size: required(fields, "orderMinSize")?', 'order_min_size: match required(fields, "orderMinSize")? { Nullable::Null => Nullable::Value(NumberOrText::Number(0.0)), other => other }', 'native nullable arm GammaMarketInfo.orderMinSize/3'),
 ('boundary.NumericEnum', 'signed_order_native', 'PolymarketSignedOrder', 'signature_type: required(fields, "signatureType")?', 'signature_type: 0.0', 'native Number bits PolymarketSignedOrder.signatureType'),
 ('boundary.NullableFlags', 'gamma_market_info_native', 'GammaMarketInfo', 'enable_order_book: optional(fields, "enableOrderBook")?', 'enable_order_book: optional(fields, "acceptingOrders")?', 'native bool GammaMarketInfo.enableOrderBook/5'),
 ('source.NumericEnumMembers', 'signed_order_native', 'constants', '  POLY_1271 = 3,', '  POLY_1271 = 4,', 'actual nine market/order/options declarations'),
 ('source.BooleanSingleton', 'gamma_market_native', 'typescript', '  /** Whether market is active for trading */\n  active: boolean;', '  /** Whether market is active for trading */\n  active: true;', 'singleton boolean literal requires distinct native type'),
]
assert len(controls)==15 and controls[-1][0]=='source.BooleanSingleton'
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':15,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','market_records','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');assert baseline.returncode==0 and '11 passed; 0 failed' in baseline.stdout,baseline.stdout
for name,test,owner,old,new,assertion in controls:
 target=typescript if owner=='typescript' else constants if owner=='constants' else source;content=ts_original if owner=='typescript' else constants_original if owner=='constants' else original
 begin,end=(0,len(content)) if owner in [None,'typescript','constants'] else span(owner)
 block=content[begin:end];assert block.count(old)==1,name
 try:
  target.write_text(content[:begin]+block.replace(old,new)+content[end:]);broken=run(name+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:target.write_text(content)
 restored=run(name+'-restored',test);summary['controls'].append({'family':name,'target':str(target),'owner':owner,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {owner}: {broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original and typescript.read_text()==ts_original and constants.read_text()==constants_original
