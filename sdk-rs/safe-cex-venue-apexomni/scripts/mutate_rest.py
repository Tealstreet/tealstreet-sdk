"""Five body controls; replacement is separate from the placement helper it invokes."""
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[3];crate=root/'sdk-rs/safe-cex-venue-apexomni';logs=root/'sdk-rs/target/apexomni-controls'
files={name:crate/'src'/name for name in ['api_runtime.rs','historical_fills.rs','trading.rs']};original={k:p.read_text()for k,p in files.items()}
controls=[('api_runtime.rs','timeout_ms: timeout,','timeout_ms: timeout + 1.0,','api_tests::api_factory_native','API factory native operations'),('historical_fills.rs','let qty = float(&quantity)?;','let qty = float(&quantity)? * 2.0;','historical_tests::historical_fills_native','fetchHistoricalFills native executions'),('trading.rs','let amount = adjust(opts.amount, p_amount);','let amount = adjust(opts.amount, p_amount) * 2.0;','trading_tests::trading_place_native','placeOrder native result'),('trading.rs','p.side = if o.side == OrderSide::Buy','p.side = if o.side == OrderSide::Sell','trading_tests::trading_cancel_symbol_native','cancelSymbolOrders native operations')]
base=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--test','exchange','--jobs','2']
try:
 for name,path in files.items():
  text=original[name]
  for file,before,after,_,_ in controls:
   if file==name:assert text.count(before)==1,(file,before);text=text.replace(before,after)
  path.write_text(text)
 p=subprocess.run(base+['--','--test-threads=1'],cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/'rest-leaf-mutation.log').write_text(out);assert p.returncode==101 and 'test result: FAILED.' in out,out
 for _,_,_,test,label in controls:assert f'test {test} ... FAILED' in out and label in out,(test,label,out)
 print('four compiled leaf controls failed101 at named native assertions',flush=True)
 for name,path in files.items():path.write_text(original[name])
 before='next.take_profit = Some(price);';assert original['trading.rs'].count(before)==1;files['trading.rs'].write_text(original['trading.rs'].replace(before,'next.take_profit = Some(price * 2.0);'))
 p=subprocess.run(base+['trading_update_native','--','--test-threads=1'],cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/'rest-update-mutation.log').write_text(out);assert p.returncode==101 and 'updateOrder native operations' in out and 'test trading_tests::trading_update_native ... FAILED' in out,out
 print('compiled replacement control failed101 at named native assertion',flush=True)
finally:
 for name,path in files.items():path.write_text(original[name])
p=subprocess.run(base+['--','--test-threads=1'],cwd=root,capture_output=True,text=True);out=p.stdout+p.stderr;(logs/'rest-restored.log').write_text(out);assert p.returncode==0,out
print('five controls restored; 34 exchange tests passed, exit 0',flush=True)
