"""Compiling controls for source filter phase and deferred enum-output refusal."""
from pathlib import Path
import os, subprocess
path=Path('sdk-rs/safe-cex-venue-woo/src/orders.rs')
original=path.read_text()
cases=[
 ('primitive-rhs', 'map_err(|error| error.message)?', 'map_err(|_error| "TypeError: algo child is not a record".to_owned())?', 'ALGO-FILTER primitive RHS exact source message'),
 ('early-type', 'let id = text(field(child, "algoOrderId")?)?;', 'let _ = required(type_hit, "type")?;\n        let id = text(field(child, "algoOrderId")?)?;', 'algo source clock count'),
 ('early-trigger', 'let id = text(field(child, "algoOrderId")?)?;', 'let _ = trigger(js_table_lookup(ORDER_TRIGGER_TYPE, &text(field(child, "triggerPriceType")?)?))?;\n        let id = text(field(child, "algoOrderId")?)?;', 'algo later source error wins'),
 ('stop-after-refusal', 'refusal.get_or_insert(error);', 'return Err(error);', 'algo source clock count'),
 ('filter-clock', 'selected.push(child);', 'let _ = now();\n        selected.push(child);', 'algo source clock count'),
 ('successful-subset', 'return Err(error);\n    }\n    Ok(orders)', 'let _ = error;\n    }\n    Ok(orders)', 'called `Result::unwrap_err()` on an `Ok` value'),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,before,after,assertion in cases:
 assert before in original,name
 try:
  mutation=original.replace(before,after,1)
  if name=='stop-after-refusal': mutation=mutation.replace('let mut refusal = None;', 'let refusal: Option<String> = None;')
  path.write_text(mutation)
  run=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','--offline','--jobs','1','-p','safe-cex-venue-woo','--test','orders','algo_refusal_preserves_source_error_priority_and_clock_counts'],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-algo-control-'+name+'.log').write_text(run.stdout)
  ok=run.returncode==101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
  print(name,run.returncode,'assertion-red' if ok else 'BAD',flush=True)
  assert ok,run.stdout
 finally:path.write_text(original)
