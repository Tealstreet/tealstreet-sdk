#!/usr/bin/env python3
"""Grouped ten-root production control plus one concrete new oracle regression."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['core.buildMarketsLowerCaseMap','core.resolveCoinToSymbol','spot.symbolOrIdToCoin','core.resolveSpotDisplayName','core.extractClearinghouseBalanceSummary','core.responseHasDeferredOrderStatus','private.getStaleCheckChannelKey','private.collectHip3DexesFromFills','core.priceToPrecision','spot.priceToPrecision']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','market_data_selectors','--jobs','2']
TAIL="const tail=call(property?(n.questionDotToken?'__optionalGet':'__get'):(n.questionDotToken?'__optionalIndex':'__index'),[inner,key]);"
def replace(s,old,new,count=1):
 assert s.count(old)==count,(old,s.count(old));return s.replace(old,new)
def section(s,start,end,old,new):
 a=s.index(start);b=s.index(end,a);return s[:a]+replace(s[a:b],old,new)+s[b:]
def observed(output,names):return[n for n in names if re.search(r'failed: SELECTOR native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',output)]
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/market_data_selectors.rs';oracle=CRATE/'scripts/market-data-selectors-oracle.cjs';original=source.read_bytes();original_oracle=oracle.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-selectors-controls-'))
 def run(label):
  r=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode();s=replace(s,'method(h, &map, "clear", &[])?;','method(h, &map, "CONTROL_CLEAR", &[])?;')
  s=section(s,'pub fn resolve_coin_to_symbol','pub fn spot_symbol_or_id_to_coin','h.get(&store, "marketsById")?','h.get(&store, "CONTROL_MARKETS")?')
  s=replace(s,'let dash = literal(h, "-");','let dash = literal(h, "|");')
  s=replace(s,'let unit = literal(h, "Unit ");','let unit = literal(h, "Bridge ");')
  s=replace(s,'h.define_data(&result, "accountValue", &account)?;','h.define_data(&result, "accountValue", &used)?;')
  s=replace(s,'h.define_data(&result, "totalMarginUsed", &used)?;','h.define_data(&result, "totalMarginUsed", &account)?;')
  s=replace(s,'"hyperliquidOrderResponseHasDeferredOrderStatus",','"CONTROL_DEFERRED",')
  s=replace(s,"units.push(b':' as u16);","units.push(b'|' as u16);")
  s=replace(s,'let set = h.global("Set")?;','let set = h.global("CONTROL_SET")?;')
  s=replace(s,'let five = h.number(5.0);','let five = h.number(6.0);')
  source.write_text(s);mutation=run('grouped-ten-production')
 finally:source.write_bytes(original)
 try:
  s=replace(original_oracle.decode(),TAIL,"const tail=call(property?'__get':'__index',[inner,key]);")
  oracle.write_text(s);regression=run('oracle-lost-second-optional-guard')
 finally:oracle.write_bytes(original_oracle)
 restored=run('restored');native_names=observed(mutation.stdout,NAMES);regression_names=observed(regression.stdout,['private.collectHip3DexesFromFills'])
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':native_names,'mutation_output':mutation.stdout,'oracle_regression':{'exit':regression.returncode,'required_native_assertion':'private.collectHip3DexesFromFills','observed_named_assertions':regression_names,'output':regression.stdout,'call_site':'transformer nested optional tail market?.info?.hip3Dex','pair':'BTC market exists, info undefined: source skips hip3Dex and yields emptySet; old transform GetV(undefined) throws; native operation next/get detects'},'restored_exit':restored.returncode,'restored_output':restored.stdout,'source_bytes_restored':source.read_bytes()==original,'oracle_bytes_restored':oracle.read_bytes()==original_oracle,'logs':str(logs),'pairs':{'summary_field_swap':[91.25,17.5],'symbol_separator':['-','|'],'display_prefix':['Unit ','Bridge '],'channel_separator':[':','|'],'significant_digit_floor':[5.0,6.0]},'iteration_stop':'tenroots/onegroupedproductioncontrol plusoneconcreteneworacle-regressioncontrol;161sourcecases through160. Actualprimitive/library/fullclass/universalbehavior outsideproof.'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 for run_result in [mutation,regression]:assert run_result.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in run_result.stdout and 'error[E' not in run_result.stdout,run_result.stdout
 assert native_names==NAMES,'survivors:'+repr(set(NAMES)-set(native_names))
 assert regression_names==['private.collectHip3DexesFromFills'],regression.stdout
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['source_bytes_restored'] and report['oracle_bytes_restored'];print('all ten native assertions and optional-chain regression discriminate; restored test passed',flush=True)
