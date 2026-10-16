"""One compiled body control per newly moved lifecycle row."""
from pathlib import Path
import subprocess,re
root=Path(__file__).resolve().parents[3];file=root/'sdk-rs/safe-cex-venue-apexomni/src/lifecycle.rs';original=file.read_text();logs=root/'sdk-rs/target/apexomni-controls'
leaf=[
 ('validate_account','String::new()','"MUTATED".into()','lifecycle_validate','validateAccount lifecycle native result'),
 ('start','bid: 0.,','bid: 1.,','lifecycle_start','start lifecycle native Store'),
 ('fetch_and_set_private_snapshot_helper','now - t < 1000.','now - t < 0.','lifecycle_snapshot_helper','fetchAndSetPrivateSnapshotHelper lifecycle native Store'),
 ('fetch_ohlcv','timestamp: numeric(data::get(r, "t"))? / 1000.,','timestamp: numeric(data::get(r, "t"))? / 100.,','lifecycle_candles','fetchOHLCV lifecycle native result'),
 ('listen_ohlcv','self.host.services()?.listen_ohlcv(opts, callback)','{ let mut opts=opts; opts.symbol.push_str("MUTATED"); self.host.services()?.listen_ohlcv(opts, callback) }','lifecycle_listen_candle','listenOHLCV lifecycle native operations'),
 ('listen_order_book','listen_order_book(symbol, callback)','listen_order_book(&format!("MUTATED{symbol}"), callback)','lifecycle_listen_book','listenOrderBook lifecycle native operations'),
 ('set_leverage','1. / leverage, 5.','1. / leverage, 4.','lifecycle_leverage','setLeverage lifecycle native operations'),
]
callers=[
 ('fetch_and_set_private_snapshot','self.fetch_and_set_private_snapshot_helper().await','{self.fetch_and_set_private_snapshot_helper().await?;self.fetch_and_set_private_snapshot_helper().await}','lifecycle_snapshot','fetchAndSetPrivateSnapshot lifecycle native notification count'),
 ('tick','self.base.get_tick_interval(),','self.base.get_tick_interval().map(|v|v+1.),','lifecycle_tick','tick lifecycle native operations'),
]
def mutate(text,method,before,after):
 m=re.search(r'    pub (?:async )?fn '+method+r'\b',text);assert m,method
 n=re.search(r'\n    (?:pub(?:\(crate\))? (?:async )?fn|fn )',text[m.end():]);end=m.end()+n.start() if n else len(text)
 part=text[m.start():end];assert part.count(before)==1,(method,before,part.count(before));return text[:m.start()]+part.replace(before,after)+text[end:]
cmd=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--test','exchange','--jobs','2','lifecycle_','--','--test-threads=1']
try:
 for group,controls in [('leaf',leaf),('caller',callers)]:
  mutant=original
  for method,before,after,_,_ in controls:mutant=mutate(mutant,method,before,after)
  file.write_text(mutant);p=subprocess.run(cmd,cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/f'lifecycle-{group}-mutation.log').write_text(out)
  assert p.returncode==101 and 'test result: FAILED.' in out,out
  for _,_,_,test,label in controls:assert f'test lifecycle_tests::{test} ... FAILED' in out and label in out,(test,label,out)
  print(f'{group}: compiled controls failed101 at each named native assertion',flush=True)
 original_wrapper=mutate(original,'sign_order','Ok(value.clone())','Ok(J::Array(vec![value.clone()]))')
 file.write_text(original_wrapper)
 wrapper_cmd=cmd[:cmd.index('--test')]+['--test','signing','--jobs','2','exchange_sign_order_native','--','--test-threads=1']
 p=subprocess.run(wrapper_cmd,cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/'lifecycle-signature-mutation.log').write_text(out)
 assert p.returncode==101 and 'exchange signOrder native result' in out and 'test exchange_sign_order_native ... FAILED' in out,out
 print('signature: compiled result control failed101 at named native assertion',flush=True)
finally:file.write_text(original)
p=subprocess.run(cmd[:cmd.index('lifecycle_')]+['--','--test-threads=1'],cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/'lifecycle-restored.log').write_text(out);assert p.returncode==0,out
p=subprocess.run(cmd[:cmd.index('--test')]+['--test','signing','--jobs','2','--','--test-threads=1'],cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/'lifecycle-signing-restored.log').write_text(out);assert p.returncode==0,out
print('10 controls restored; 27 exchange and 7 signing tests passed, exit 0',flush=True)
