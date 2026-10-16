"""Eight compiled, isolated production-body faults; retain wrong failures/survivors."""
from pathlib import Path
import json, os, re, subprocess
crate=Path(__file__).resolve().parents[1]
source=crate/'src/public_subscriptions.rs'
logs=Path(os.environ['MEXC_PUBLIC_CONTROL_LOG_DIR']); logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/mexc')],text=True).strip(),'commit gate inputs first'
original=source.read_text()
controls=[
 ('connectAndSubscribe','connect_native','connect','if host.disposed() {','if { let _ = host.disposed(); false } {','native independent trace count connectAndSubscribe/1'),
 ('listenOHLCV','candle_native','listen_ohlcv','host.subscribe(subscription.clone())?;','let _ = &subscription;','native independent trace count listenOHLCV/0'),
 ('handleActiveTickerSymbolsChanged','active_native','active_tickers_changed','host.reconcile()','Ok(())','native independent trace count handleActiveTickerSymbolsChanged/0'),
 ('reconcileTickerSubscriptions','ticker_native','reconcile_tickers','.take(100)','.take(99)','native independent trace count reconcileTickerSubscriptions/10'),
 ('resubscribe','resubscribe_native','resubscribe','["subscriptions", "tickerSubscriptions"]','["subscriptions"]','native independent trace count resubscribe/0'),
 ('sendWithDiagnostics','send_diagnostics_native','send_diagnostics','DiagnosticHost::send(&*host, data)?;','let _ = data;','native independent trace count sendWithDiagnostics/0'),
 ('emitReceiveDiagnostics','receive_diagnostics_native','receive_diagnostics','format.global_string(&message)?','format.abstract_string(&message)?','native formatter expected failure'),
 ('handleRoutedMessage','route_native','route_message','while let Some(handler) = host.next_handler(&mut iterator)? {','let mut seen = 0;\n    while let Some(handler) = host.next_handler(&mut iterator)? {\n        if seen == 2 { break; }\n        seen += 1;','native independent trace count handleRoutedMessage/8'),
]
assert len(controls)==8 and controls[-1][0]=='handleRoutedMessage'
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':8,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-mexc','--test','public_subscriptions','--locked','--jobs','2','--']
 if test: cmd.extend(['--exact',test])
 cmd.append('--nocapture')
 r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout)
 return r
baseline=run('baseline'); summary['baseline_exit']=baseline.returncode
(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
assert baseline.returncode==0 and '8 passed; 0 failed' in baseline.stdout,baseline.stdout
for method,test,function,old,new,assertion in controls:
 start=re.search(r'\npub fn '+re.escape(function)+r'\b',original); assert start,function
 begin=start.start(); following=re.search(r'\npub fn ',original[start.end():]); end=start.end()+following.start() if following else len(original)
 body=original[begin:end]; assert body.count(old)==1,method
 try:
  source.write_text(original[:begin]+body.replace(old,new)+original[end:])
  broken=run(method+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:
  source.write_text(original)
 restored=run(method+'-restored',test)
 summary['controls'].append({'method':method,'function':function,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught})
 summary['floor']=f"{len(summary['controls'])} controls executed; stopped after {method}"
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {function}: {broken.stdout}'
 print(f'{method}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
