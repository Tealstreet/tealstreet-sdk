#!/usr/bin/env python3
"""One grouped production control for the15new rows; restores before rerun."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
MODELS=['AllMids','UserRateLimit','OrderStatus','HyperliquidFastAssetCtx','HyperliquidFastAssetCtxs','PrePlacementEntry','PendingOrderEntry','AgentWalletCreationResult']
FUNCTIONS=['parseJsonObject','stringifyDiagnosticPayload','formatHyperliquidWsDiagnostic','shouldEmitHyperliquidWsSendDiagnostic','shouldEmitHyperliquidWsReceiveDiagnostic','maskHyperliquidAddress','parseHyperliquidExchangePayload']
CARGO=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','json_boundaries','--jobs','2']
assert len(MODELS)==8 and len(FUNCTIONS)==7
WRAPPERS='''
pub fn parse_json_object<H:JsPayloadHost>(host:&mut H,data:&H::Value)->Result<Option<H::Value>,H::Error>{original_parse_json_object(host,data).map(|_|None)}
pub fn parse_hyperliquid_exchange_payload<H:JsPayloadHost>(host:&mut H,data:&H::Value)->Result<Option<H::Value>,H::Error>{original_parse_hyperliquid_exchange_payload(host,data).map(|_|None)}
pub fn stringify_diagnostic_payload<H:JsPayloadHost>(host:&mut H,data:&H::Value)->Result<JsString,H::Error>{original_stringify_diagnostic_payload(host,data).map(|mut v|{v.0.push(1);v})}
pub fn format_hyperliquid_ws_diagnostic<H:JsPayloadHost>(host:&mut H,scope:&JsString,data:&H::Value)->Result<JsString,H::Error>{original_format_hyperliquid_ws_diagnostic(host,scope,data).map(|mut v|{v.0.push(1);v})}
pub fn mask_hyperliquid_address<H:JsPayloadHost>(host:&H,data:&H::Value)->Option<JsString>{original_mask_hyperliquid_address(host,data).map(|mut v|{v.0.push(1);v})}
pub fn should_emit_hyperliquid_ws_send_diagnostic<H:JsPayloadHost>(host:&mut H,data:&H::Value)->Result<bool,H::Error>{original_should_emit_hyperliquid_ws_send_diagnostic(host,data).map(|v|!v)}
pub fn should_emit_hyperliquid_ws_receive_diagnostic<H:JsPayloadHost>(host:&mut H,scope:&JsString,data:&H::Value)->Result<bool,H::Error>{original_should_emit_hyperliquid_ws_receive_diagnostic(host,scope,data).map(|v|!v)}
'''

def run(command,label,env=None):
 result=subprocess.run(command,cwd=ROOT,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(label+'.log')).write_text(result.stdout);print(f'{label}:exit{result.returncode}',flush=True);return result.returncode,result.stdout

def save():args.output.write_text(json.dumps(report,indent=2)+'\n')

if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
 sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 logs=Path(tempfile.mkdtemp(prefix='hyperliquid-json-control-'))
 report={'measured_sha':sha,'cargo_command':CARGO,'logs':str(logs),'controls':[],'scope':'one grouped15new-row native control; no historical/exhaustive matrix claim'};save()
 codec=CRATE/'src/spot_json.rs';boundaries=CRATE/'src/json_boundaries.rs';original={p:p.read_bytes() for p in [codec,boundaries]}
 try:
  text=original[codec].decode();anchor='        let units = string_token(raw.get()).map_err(serde::de::Error::custom)?;';assert text.count(anchor)==1
  text=text.replace(anchor,'        let mut units = string_token(raw.get()).map_err(serde::de::Error::custom)?;\n        if let Some(first)=units.first_mut(){*first^=1;}')
  anchor='    number_token(raw.get()).map_err(serde::de::Error::custom)';assert text.count(anchor)==1
  text=text.replace(anchor,'    number_token(raw.get()).map(|number|number+1.0).map_err(serde::de::Error::custom)');codec.write_text(text)
  text=original[boundaries].decode()
  for name in ['parse_json_object','stringify_diagnostic_payload','format_hyperliquid_ws_diagnostic','should_emit_hyperliquid_ws_send_diagnostic','should_emit_hyperliquid_ws_receive_diagnostic','mask_hyperliquid_address','parse_hyperliquid_exchange_payload']:
   anchor='pub fn '+name+'<';assert text.count(anchor)==1;text=text.replace(anchor,'fn original_'+name+'<')
  boundaries.write_text(text+WRAPPERS)
  code,output=run(CARGO,'grouped15production-mutation')
 finally:
  for p,data in original.items():p.write_bytes(data)
 restored,restored_output=run(CARGO,'restored')
 names=['JSONMODEL native '+name+' from_str mode 0' for name in MODELS]+['JSONBOUND native result '+name for name in FUNCTIONS]
 report['controls'].append({'name':'grouped15production-mutation','exit':code,'required_named_assertions':names,'observed_named_assertions':[n for n in names if n in output],'restored_exit':restored,'restored_bytes_equal':all(p.read_bytes()==data for p,data in original.items())});save()
 assert code==101 and 'error[E' not in output,output
 assert report['controls'][0]['observed_named_assertions']==names,'survivor(s):'+repr(set(names)-set(report['controls'][0]['observed_named_assertions']))
 assert restored==0 and '3 passed; 0 failed' in restored_output,restored_output
 assert report['controls'][0]['restored_bytes_equal']
 env={**os.environ,'HYPERLIQUID_HERITAGE_SOURCE':'hyperliquidspot.types.ts'}
 for name in ['json-models-generate','json-models-oracle']:
  command=['node','--require',str(CRATE/'scripts/heritage-control.cjs'),str(CRATE/'scripts'/(name+'.cjs')),'--check']
  code,output=run(command,name+'-heritage',env);report['controls'].append({'name':name+'-heritage','exit':code,'named_guard':'Unsupported interface heritage:','observed':'Unsupported interface heritage:' in output});save();assert code!=0 and 'Unsupported interface heritage:' in output,output
 report['iteration_stop']='8model and7function named native failures;2newmodel heritage entrypoints; all production files restored';save();print('all15new rows discriminate; restored3tests pass',flush=True)
