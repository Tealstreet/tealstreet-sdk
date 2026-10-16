"""Sequential compiling controls, restoring the actual production source."""
from pathlib import Path
import subprocess,os
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitrue/src/leverage_reads.rs';original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','leverage_reads','--locked','--jobs','1']
controls=[
 ('sync wrong maximum','set_market_max_leverage(store, symbol, maximum)?','set_market_max_leverage(store, symbol, maximum + 1.0)?','BITRUE-LEVERAGE-READ committed native fields'),
 ('wrong fallback','_ => 1.0,','_ => 2.0,','BITRUE-LEVERAGE-READ result'),
 ('unawaited refresh','fetch_and_sync_symbol_max_leverage(store, symbol, public_data_only, get).await;','std::mem::drop(fetch_and_sync_symbol_max_leverage(store, symbol, public_data_only, get));','BITRUE-LEVERAGE-READ native request'),
 ('source catch fabricated success','Err(_source_catch_discard) => None','Err(_source_catch_discard) => Some(99.0)','BITRUE-LEVERAGE-READ result'),
 ('wrong request key','"contractName":symbol','"symbol":symbol','BITRUE-LEVERAGE-READ native request'),
]
for label,old,new,assertion in controls:
 assert original.count(old)==1,(label,old)
 try:
  source.write_text(original.replace(old,new));result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode==101 and assertion in output and 'could not compile' not in output,output
  print('COMPILED EXIT101 '+label+' -> '+assertion,flush=True)
 finally:source.write_text(original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('RESTORED PASS '+label,flush=True)
