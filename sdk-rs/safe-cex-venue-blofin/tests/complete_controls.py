"""One compound production mutation, eight named runtime failures, guaranteed restoration.
This is aggregate discrimination; no claim of independent mutation completeness.
"""
from pathlib import Path
import subprocess,json,os,hashlib,sys
root=Path(__file__).resolve().parents[3];crate=root/'sdk-rs/safe-cex-venue-blofin';sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();out=Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
private=crate/'src/private_stream_data.rs';exchange=crate/'src/exchange.rs';original={p:p.read_bytes()for p in [private,exchange]}
mutations=[
 (private,'let delta = math_max(filled - previous, 0.0);','let delta = math_max(filled - previous, 0.0) * 2.0;','fill delta'),
 (private,'.update_orders_status(&ids, status)','.update_orders_status(&ids, { let _ = status; OrderStatus::Open })','terminal status reconciliation'),
 (private,'balance.used = if num_truth(used) { used } else { 0.0 };','balance.used = if num_truth(used) { used + 1.0 } else { 0.0 };','position used balance'),
 (private,'balance.free = subtract(total, host.store().balance().used);','balance.free = subtract(total, host.store().balance().used) + 1.0;','account free balance'),
 (private,'core::on_message(self.bridge.as_ref(), data)','core::on_message(self.bridge.as_ref(), if data.is_empty() { data } else { "pong" })','private facade dispatch'),
 (exchange,'name: name.clone(),','name: ExchangeName::Binance,','exchange facade context name'),
 (exchange,'socket.listen_candles(opts, callback)','{ drop(callback); socket.listen_candles(opts, Rc::new(|_| Ok(()))) }','OHLCV callback identity'),
 (exchange,'socket.listen_book(symbol, callback)','{ drop(callback); socket.listen_book(symbol, Rc::new(|_| Ok(()))) }','book callback identity'),
]
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','complete','--jobs','2'];env=os.environ.copy();env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
report={'baseSha':sha,'mode':'compound; no independent per-mutation completeness','command':command,'mutations':[{'path':str(p.relative_to(root)),'before':a,'after':b,'callSite':label}for p,a,b,label in mutations]}
expected=['terminal_reconciliation_native','orders_native','positions_native','account_native','private_facade_native','exchange_class_native','forwarding_ohlcv_native','forwarding_book_native']
def run(label):
 with (out/(label+'.log')).open('w')as log:r=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 return r.returncode,(out/(label+'.log')).read_text()
try:
 for p,a,b,label in mutations:
  text=p.read_text();assert text.count(a)==1,(label,'mutation anchor floor1');p.write_text(text.replace(a,b))
 report['mutatedSha256']={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in original}
 code,log=run('negative');report['negativeExit']=code;report['failedTests']=[name for name in expected if f'test {name} ... FAILED' in log];report['compiledRuntime']='Finished `test`' in log and 'running 8 tests' in log
 assert code==101 and report['compiledRuntime'],'compiler rejection is not discrimination'
 assert report['failedTests']==expected,report['failedTests']
finally:
 for p,data in original.items():p.write_bytes(data)
 report['restoredSha256']={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in original}
 report['byteRestored']=all(p.read_bytes()==data for p,data in original.items());(out/'controls.json').write_text(json.dumps(report,indent=2)+'\n')
code,log=run('restored');report['restoredExit']=code;report['passedTests']=[name for name in expected if f'test {name} ... ok' in log];report['scopedDiffExit']=subprocess.run(['git','diff','--exit-code',sha,'--','sdk-rs/safe-cex-venue-blofin/src'],cwd=root).returncode
(out/'controls.json').write_text(json.dumps(report,indent=2)+'\n');assert code==0 and report['passedTests']==expected and report['scopedDiffExit']==0,'restoration gate'
print(json.dumps(report,indent=2))
