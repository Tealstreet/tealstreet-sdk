"""Run alone: compiled production faults, named RED, exact restoration GREEN."""
import argparse,os,pathlib,subprocess
sdk=pathlib.Path(__file__).resolve().parents[2]
os.chdir(sdk)
os.environ['CARGO_INCREMENTAL']='0'
logs=sdk/'target/polymarket-eager-controls';logs.mkdir(parents=True,exist_ok=True)
wrapper=sdk/'safe-cex-venue-polymarket/src/binding/exchange.rs'
api=sdk/'safe-cex-venue-polymarket/src/binding/members.rs'
originals={p:p.read_bytes() for p in (wrapper,api)}
text=originals[wrapper].decode()
typed='sdk_typed_prefix_and_dropped_waiter_retain_real_market_continuation'
unit='sdk_unit_warning_precedes_waiter_poll_and_drop'
raw='live_server_time_keeps_eager_calls_raw_values_and_original_failures'
old='        let result = self.rt.this.call_method(member, args);\n        Box::pin(async move {'
new='        let this = self.rt.this.clone();\n        Box::pin(async move {\n            let result = this.call_method(member, args);'
assert text.count(old)==2
cases=[
 ('typed-prefix',wrapper,text.replace(old,new,1),typed,'POLY-SDK-TYPED-PREFIX'),
 ('unit-prefix',wrapper,text[:text.index('    fn invoke_unit')]+text[text.index('    fn invoke_unit'):].replace(old,new,1),unit,'POLY-SDK-UNIT-PREFIX'),
 ('time-endpoint',api,originals[api].decode().replace('"getServerTime" => endpoint("TIME")?','"getServerTime" => endpoint("MARKETS")?'), 'class_cost_http_native','class_cost_http_url'),
 ('time-result-alias',api,originals[api].decode().replace('    Ok(data)\n','    if n == "getServerTime" && matches!(data, Js::Object(_)) { return data.spread_object(); }\n    Ok(data)\n'),raw,'time raw identity:18'),
 ('time-error-alias',api,originals[api].decode().replace('    .await?;\n    if discard','    .await.map_err(|e| Thrown::new(e.value.spread_object().unwrap()))?;\n    if discard'),raw,'time original failure identity'),
]
parser=argparse.ArgumentParser();parser.add_argument('--only',choices=[c[0] for c in cases]);args=parser.parse_args()
if args.only:cases=[c for c in cases if c[0]==args.only]
def run(name,test):
 result=subprocess.run(['cargo','test','--locked','--jobs','2','-p','safe-cex-venue-polymarket','--lib',test,'--','--nocapture'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (logs/(name+'.log')).write_text(result.stdout)
 return result.returncode,result.stdout
try:
 for name,path,mutated,test,needle in cases:
  assert mutated.encode()!=originals[path],name
  path.write_text(mutated)
  code,out=run(name+'-red',test)
  assert code==101 and 'Finished `test`' in out and needle in out and 'error[E' not in out,(name,code,out[-3500:])
  print(name,'compiled named RED',flush=True)
  path.write_bytes(originals[path]);assert path.read_bytes()==originals[path]
  code,out=run(name+'-restored-green',test)
  assert code==0,(name,out[-3500:])
  print(name,'exact restore GREEN',flush=True)
finally:
 for path,data in originals.items():path.write_bytes(data)
