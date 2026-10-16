"""Sequential compiled controls; restore original source in finally, never mutate metadata."""
import json, os, pathlib, subprocess, sys
root=pathlib.Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-coinbase/src/public_stream_dispatch.rs'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
out=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else '/tmp/coinbase-public-stream-controls')/sha
out.mkdir(parents=True,exist_ok=True)
original=source.read_text()
controls=[
 ('lifecycle-endpoint','wss://advanced-trade-ws.coinbase.com','wss://broken-native-control.invalid','connect-live: public endpoint'),
 ('candle-open-close','candle.get("open")','candle.get("close")','candles-grouped-aliases.open: decoded native number bits'),
 ('first-trade','trades.borrow().first().cloned()','trades.borrow().last().cloned()','trades-first-only-raw-symbol: native UTF16 string'),
 ('book-raw-topic','handler(r, &format!("level2.{symbol}"))','handler(r, &format!("level2.WRONG-{symbol}"))','books-route-all-event-types: independent source invocation count'),
]
if len(sys.argv)>2:
 controls=[c for c in controls if c[0]==sys.argv[2]]
 assert len(controls)==1, 'named control selection must identify one declared family'
cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','public_stream_dispatch','public_stream_live_native_corpus','--jobs','2','--','--exact']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results=[]
def run(name,stage):
 with (out/(name+'-'+stage+'.log')).open('w') as f:r=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 return r.returncode,(out/(name+'-'+stage+'.log')).read_text()
try:
 for name,old,new,assertion in controls:
  assert original.count(old)==1,(name,'mutation selector must be unique')
  source.write_text(original.replace(old,new))
  code,text=run(name,'broken')
  source.write_text(original)
  restored,restore_text=run(name,'restored')
  result=dict(name=name,call_site=old,named_assertion=assertion,broken_exit=code,restored_exit=restored,compiled='Finished `test`' in text and 'error[E' not in text,named_failure=assertion in text,broken_output=text,restored_output=restore_text)
  results.append(result)
  (out/'results.json').write_text(json.dumps(dict(sha=sha,command=cmd,enumeration='stopped after '+str(len(results))+' selected controls; four family controls declared',controls=results),indent=2)+'\n')
  print(json.dumps({k:v for k,v in result.items() if not k.endswith('output')}),flush=True)
  if not(code==101 and restored==0 and result['compiled'] and result['named_failure']):raise SystemExit('Control failed or survived; result retained as finding: '+name)
finally:source.write_text(original)
assert source.read_text()==original
