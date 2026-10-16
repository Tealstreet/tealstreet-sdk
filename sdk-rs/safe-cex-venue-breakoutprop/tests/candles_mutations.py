"""One Node corpus per target; all controls compile, fail by name, restore/pass."""
from pathlib import Path
import os,subprocess,re,argparse
from decoder_mutations import replace_decoder_field
parser=argparse.ArgumentParser();parser.add_argument("--start-at",type=int,default=0);args=parser.parse_args()
crate=Path(__file__).resolve().parents[1];source=crate/'src/candles.rs';test=crate/'tests/candles.rs';types=crate/'src/types.rs'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','candles','--jobs','1','actual_candle_read','--','--exact']
controls=[('timestamp scale','/ 1000.0','/ 1.0','CANDLE-NATIVE decoded fields')]
controls += [(field+' swap',f'{field}: raw.{field}',f'{field}: raw.{other}','CANDLE-NATIVE decoded fields') for field,other in [('open','close'),('high','low'),('low','high'),('close','open'),('volume','open')]]
controls += [('from to swap','date(options.from)?','date(options.to)?','CANDLE-REQUEST'),('endpoint','url: "marketdata".into()','url: "wrong".into()','CANDLE-REQUEST'),('flatten original metadata','        result\n','        result.map_err(|e| { let mut flat=ExchangeError::new(e.message); flat.name=e.name; flat })\n','CANDLE-ERROR original metadata')]
for shape,expression in [('empty','{drop(candles);vec![]}'),('subset','candles.into_iter().take(1).collect()'),('extra','{ let mut rows: Vec<Candle> = candles; if let Some(c)=rows.first().cloned(){rows.push(c);} rows }')]:
 controls.append((shape,'Ok(candles)',f'Ok({expression})','CANDLE-CARDINALITY independent native count'))
controls.append(('consuming decoder','','','CANDLE-CARDINALITY independent native count'))
controls.append(('consuming decoder guards bypassed','','','CANDLE-NATIVE decoded fields'))
for name,old,new,assertion in controls[args.start_at:]:
 originals={p:p.read_bytes() for p in [source,test,types]}
 try:
  if name.startswith('consuming'):
   s=originals[types].decode();s=replace_decoder_field(s,'    pub events: Vec<CandleEvent>,','    #[serde(deserialize_with="consume_all")]\n    pub events: Vec<CandleEvent>,')
   s+='\nfn consume_all<\'de,D:serde::Deserializer<\'de>>(d:D)->Result<Vec<CandleEvent>,D::Error>{let consumed=Vec::<CandleEvent>::deserialize(d)?;drop(consumed);Ok(vec![])}\n';types.write_text(s)
  else:
   s=originals[source].decode();assert s.count(old)==1,(name,s.count(old));s=s.replace(old,new).replace('let candles = data','let candles: Vec<Candle> = data');source.write_text(s)
  if name.endswith('guards bypassed'):
   s=originals[test].decode();s=re.sub(r'assert_eq!\(\s*case\["expected"\]\["ok"\].*?"CANDLE-SOURCE per-case count"\s*\);','',s,flags=re.S)
   s=re.sub(r'assert_eq!\(\s*rows.len\(\),\s*count,\s*"CANDLE-CARDINALITY independent native count"\s*\);','',s,flags=re.S);test.write_text(s.replace('let count =', 'let _count ='))
  r=subprocess.run(cmd,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and assertion in out and 'test result: FAILED' in out,(name,r.returncode,out)
  print(f'{name}: compiled; exit101; {assertion}',flush=True)
 finally:
  for p,b in originals.items():p.write_bytes(b)
 r=subprocess.run(cmd,env=env,capture_output=True,text=True);assert r.returncode==0,(name,r.returncode,r.stdout+r.stderr)
 print(f'{name}: restored PASS',flush=True)
