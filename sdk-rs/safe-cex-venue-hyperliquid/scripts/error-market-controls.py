#!/usr/bin/env python3
"""One grouped native production control for12new rows; always restore sources."""
import argparse,json,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
ERROR_NAMES=['core.class','core.constructor','core.handleApiError','spot.class','spot.constructor','spot.handleApiError']
HELPERS=['symbolToCoin','calculatePricePrecision','parseHyperliquidVolumeFields','parseHyperliquidOpenInterestNotional','decodeHyperliquidFastAssetCtxs','parseMetaAndAssetCtxsResponse']
assert len(ERROR_NAMES)==len(HELPERS)==6
CARGO=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','error_market_helpers','--jobs','2']
WRAPPERS='''
pub fn symbol_to_coin<H:MarketScalarHost>(host:&mut H,symbol:&JsString)->Result<JsString,H::Error>{control_original_symbol_to_coin(host,symbol).map(|mut s|{s.0.push(1);s})}
pub fn calculate_price_precision<H:MarketScalarHost>(host:&mut H,price:f64,amount:f64,max:f64)->Result<f64,H::Error>{control_original_calculate_price_precision(host,price,amount,max).map(|v|v+1.0)}
pub fn parse_hyperliquid_volume_fields<H:MarketScalarHost>(host:&mut H,ticker:&H::Value,price:f64)->Result<VolumeFields,H::Error>{control_original_parse_hyperliquid_volume_fields(host,ticker,price).map(|mut v|{v.volume+=1.0;v})}
pub fn parse_hyperliquid_open_interest_notional<H:MarketScalarHost>(host:&mut H,ticker:&H::Value,price:f64)->Result<f64,H::Error>{control_original_parse_hyperliquid_open_interest_notional(host,ticker,price).map(|v|v+1.0)}
pub fn decode_hyperliquid_fast_asset_ctxs<H:FastAssetCtxHost>(host:&mut H,payload:&H::Value)->Result<H::Value,H::Error>{control_original_decode_hyperliquid_fast_asset_ctxs(host,payload).map(|_|host.undefined())}
pub fn parse_meta_and_asset_ctxs_response<H:MetaTupleHost>(host:&mut H,data:&H::Value,source:&JsString,context:&JsString)->Result<Option<[H::Value;2]>,H::Error>{control_original_parse_meta_and_asset_ctxs_response(host,data,source,context).map(|_|None)}
'''
def run(label):
 result=subprocess.run(CARGO,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(result.stdout);print(f'{label}:exit{result.returncode}',flush=True);return result.returncode,result.stdout
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 logs=Path(tempfile.mkdtemp(prefix='hyperliquid-error-market-controls-'));files=[CRATE/'src/api_errors.rs',CRATE/'src/market_helpers.rs'];original={f:f.read_bytes() for f in files}
 try:
  text=original[files[0]].decode();anchor='host.literal("HyperliquidAPIError")';assert text.count(anchor)==1;text=text.replace(anchor,'host.literal("CONTROL_ERROR_NAME")');files[0].write_text(text)
  text=original[files[1]].decode()
  for name in ['symbol_to_coin','calculate_price_precision','parse_hyperliquid_volume_fields','parse_hyperliquid_open_interest_notional','decode_hyperliquid_fast_asset_ctxs','parse_meta_and_asset_ctxs_response']:
   anchor='pub fn '+name+'<';assert text.count(anchor)==1;text=text.replace(anchor,'fn control_original_'+name+'<')
  files[1].write_text(text+WRAPPERS);code,output=run('grouped12production-mutation')
 finally:
  for file,data in original.items():file.write_bytes(data)
 restored,positive=run('restored')
 names=['HELPER native receiver/argument '+n+' set:name' for n in ERROR_NAMES]+['HELPER native result '+n for n in HELPERS]
 observed=[n for n in names if n in output]
 report={'measured_sha':sha,'cargo_command':CARGO,'logs':str(logs),'mutation_exit':code,'required_named_native_assertions':names,'observed_named_native_assertions':observed,'restored_exit':restored,'restored_bytes_equal':all(f.read_bytes()==data for f,data in original.items()),'iteration_stop':'6Error class/constructor/handler native name-argument assertions and6helper decoded native result assertions; one grouped control, no exhaustive historical claim'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert code==101 and 'error[E' not in output,output
 assert observed==names,'survivor(s):'+repr(set(names)-set(observed))
 assert restored==0 and '2 passed; 0 failed' in positive,positive
 assert report['restored_bytes_equal'];print('all12new native assertions discriminate; restored2tests pass',flush=True)
