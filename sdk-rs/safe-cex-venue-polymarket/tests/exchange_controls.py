"""Run compiled controls serially; every receipt names the unmodified input SHA."""
from pathlib import Path
import json, os, shutil, subprocess
crate=Path(__file__).resolve().parents[1];root=crate.parent.parent
source=crate/'src/exchange.rs';original=source.read_text();logs=Path(os.environ['POLYMARKET_CONTROL_DIR']);logs.mkdir(parents=True,exist_ok=True)
cargo=shutil.which('cargo');assert cargo and os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate/'src'),str(crate/'tests')],text=True).strip(),'commit input before controls'
controls=[
 ('class-maps','class_native','let map = host.construct(ctor, vec![])?;','let map = if key=="costBasis"{host.field(&receiver,"marketMetadata")?}else{host.construct(ctor,vec![])?};','native independent maps'),
 ('constructor-default','constructor_native','("signerType", host.number(0.))','("signerType", host.number(1.))','constructor default EOA'),
 ('market-collateral','markets_native','("quote", h.text("pUSD"))','("quote", h.text("USD"))','prediction collateral'),
 ('ticker-bid','tickers_native','("bid", price.clone())','("bid", h.number(0.))','native bits bid'),
 ('price-axis','place_native','("price", price.clone()),\n                    ("size", size.clone()),','("price", size.clone()),\n                    ("size", price.clone()),','price tick axis'),
 ('amount-axis','place_native','let size_string = h.fixed(&h.field(&opts, "amount")?, &precision)?;','let _=precision;let size_string = h.fixed(&h.field(&opts,"amount")?, &h.number(2.))?;','amount decimal axis'),
 ('replacement-presence','update_native','if h.has(&update, "amount")?','if false && h.has(&update, "amount")?','explicit undefined overrides amount'),
 ('history-time','ohlcv_native','h.to_number(&h.field(&row, "t")?)? * 1000.','h.to_number(&h.field(&row, "t")?)? * 1.','candle milliseconds'),
 ('poll-duplicates','listen_native','h.greater(&h.field(&candle, "timestamp")?, &last.borrow())?','h.greater_equal(&h.field(&candle, "timestamp")?, &last.borrow())?','strictly newer candle emissions'),
]
summary={'sha':sha,'declared_floor':18,'controls':[]}
def run(name,test=None):
 cmd=[cargo,'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-polymarket','--test','exchange','--locked','--jobs','1','--']
 if test:cmd+=['--exact',test]
 r=subprocess.run(cmd,cwd=root,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 output=r.stdout.replace(str(root),'<worktree>').replace(str(Path.home()),'<home>');(logs/(name+'.log')).write_text('exit='+str(r.returncode)+'\n'+output);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;assert baseline.returncode==0 and '19 passed; 0 failed' in baseline.stdout
extra=[
 ('factory-rate','completion::factory_native','src/api_factory.rs','last_request.set(h.date_now()?);','let _=h.date_now()?;','native shared client rate limiter'),
 ('api-class-prototype','completion::api_class_native','src/classes.rs','            "getSpread",\n','','native independent API prototype floor'),
 ('api-credential-reference','completion::api_credentials_native','src/classes.rs','Rc::new(move || h.field(&r, "getCredentials"))','Rc::new(move || {let _=&r;Ok(h.undefined())})','native API callback property reference retains function alias'),
 ('public-class-policy','completion::public_class_native','src/classes.rs','host.boolean(false)','host.boolean(true)','native socket offline recovery policy'),
 ('private-class-policy','completion::private_class_native','src/classes.rs','h.boolean(false)','h.boolean(true)','native socket offline recovery policy'),
 ('private-credential-reference','completion::private_credentials_native','src/classes.rs','Rc::new(move || h.field(&r, "getCredentials"))','Rc::new(move || {let _=&r;Ok(h.undefined())})','native private callback property reference retains function alias'),
 ('price-field-axis','completion::price_native','src/classes.rs','parse_field(h, &change, "best_bid")','parse_field(h, &change, "best_ask")','native best bid'),
 ('book-shared-alias','completion::book_native','src/classes.rs','host.apply(callback, vec![book.clone()])?;','host.apply(callback, vec![host.spread(&book)?])?;','native shared order book callback alias'),
 ('source-control-noop','source_oracle_guard_rejects_price_mutation','tests/exchange_oracle.cjs',"if(process.env.POLYMARKET_SOURCE_CONTROL==='price-axis')","if(false&&process.env.POLYMARKET_SOURCE_CONTROL==='price-axis')",'source guard must reject production source mutation'),
]
expanded=[(name,test,'src/exchange.rs',old,new,label) for name,test,old,new,label in controls]+extra
for name,test,relative,old,new,label in expanded:
 source=crate/relative;original=source.read_text()
 assert original.count(old)==1,(name,relative,original.count(old))
 try:
  source.write_text(original.replace(old,new));broken=run(name+'-mutation',test)
 finally:source.write_text(original)
 restored=run(name+'-restored',test)
 caught=broken.returncode==101 and label in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 summary['controls'].append({'family':name,'call_site':relative,'test':test,'assertion':label,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['stop']=name;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(name,broken.returncode,restored.returncode,'caught='+str(caught),flush=True)
 assert caught,'FINDING: survivor, compile failure or wrong named assertion: '+name
 assert restored.returncode==0,name
assert source.read_text()==original
