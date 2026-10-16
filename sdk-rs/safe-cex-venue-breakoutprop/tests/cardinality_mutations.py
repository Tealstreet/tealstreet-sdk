"""Empty and proper-subset production controls, one Node corpus per target run."""
from pathlib import Path
import os, subprocess, argparse
parser=argparse.ArgumentParser();parser.add_argument("--only",default="");parser.add_argument("--start-at",type=int,default=0);args=parser.parse_args()
crate=Path(__file__).resolve().parents[1]
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('parity','types.rs','serde_json::Value::Array(items) => Ok(serde_json::Value::Array(items)),','serde_json::Value::Array(mut items) => { ACTION Ok(serde_json::Value::Array(items)) },','native_fields_and_bits'),
 ('portfolio','portfolio.rs','    Ok(positions)','    ACTION\n    Ok(positions)','portfolio_formatPositions'),
 ('portfolio_reads','portfolio.rs','    Ok(positions)','    ACTION\n    Ok(positions)','portfolio_read_fetchPositions'),
 ('raw_reads','raw_reads.rs','    js_from_value(json!({key:filtered}))','    ACTION\n    js_from_value(json!({key:filtered}))','raw_read_fetchInstrumentsRaw'),
]
for target, prefix in [('portfolio', 'portfolio_'), ('portfolio_reads', 'portfolio_read_')]:
 for variable,method in [('positions','formatOpenPositions' if target=='portfolio' else 'fetchOpenPositions'), ('orders','formatOrders' if target=='portfolio' else 'fetchOrders')]:
  controls.append((target,'portfolio.rs',f'    Ok(Some({variable}))',f'    ACTION\n    Ok(Some({variable}))',prefix+method))
controls.append(('raw_reads','raw_reads.rs','    js_from_value(json!({key:filtered}))','    if key == "instrumentDetails" { ACTION }\n    js_from_value(json!({key:filtered}))','raw_read_fetchInstrumentDetailsRaw'))
portfolio_template = '''let mut raw = input.decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value })).unwrap().value;
                if let Some(rows) = raw.get_mut("portfolios").and_then(Value::as_array_mut) { ACTION }
                js_from_value(raw).map_err(|e| ExchangeError::new(e.to_string()))'''
controls.append(('raw_reads','raw_reads.rs','Ok(input)',portfolio_template,'raw_read_fetchPortfolioRaw'))
for control_index,(target,file,old,template,named) in enumerate(controls):
 if control_index < args.start_at: continue
 if args.only and args.only not in named: continue
 source=crate/'src'/file
 variable = 'items' if target=='parity' else 'rows' if old=='Ok(input)' else 'orders' if 'Some(orders)' in old else 'filtered' if file=='raw_reads.rs' else 'positions'
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test',target,'--jobs','1','--',{'parity':'native_fields_and_bits','portfolio':'actual_portfolio_formatters','portfolio_reads':'actual_portfolio_reads','raw_reads':'actual_raw_reads'}[target],'--exact']
 for kind,action in [('empty',f'{variable}.clear();'),('subset',f'{variable}.truncate(1);'),('extra',f'if let Some(extra) = {variable}.first().cloned() {{ {variable}.push(extra); }}')]:
  original=source.read_bytes();text=original.decode();assert text.count(old)==1
  guard=crate/'tests/support/cardinality.rs';guard_original=guard.read_bytes()
  try:
   guard_text=guard_original.decode();assert guard_text.count('assert_eq!(source_census(value), expected,')==1
   guard.write_text(guard_text.replace('assert_eq!(source_census(value), expected,','assert_eq!({ let _ = source_census(value); expected.clone() }, expected,'))
   source.write_text(text.replace(old,template.replace('ACTION',action)))
   result=subprocess.run(command,env=env,capture_output=True,text=True);out=result.stdout+result.stderr
   assert result.returncode==101 and named in out and 'NATIVE-CARDINALITY' in out and 'test result: FAILED' in out,(target,kind,result.returncode,out)
   print(f'{target} {kind}: source census bypassed; compiled; exit 101; NATIVE-CARDINALITY {named}',flush=True)
  finally:
   source.write_bytes(original)
   guard.write_bytes(guard_original)
  result=subprocess.run(command,env=env,capture_output=True,text=True)
  assert result.returncode==0,(target,kind,result.stdout+result.stderr)
  print(f'{target} {kind}: restored PASS',flush=True)
