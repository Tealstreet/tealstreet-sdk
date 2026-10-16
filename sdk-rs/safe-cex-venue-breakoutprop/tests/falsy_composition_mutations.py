"""Each wrapper must select stored fallbacks for successful falsy raw bodies."""
from pathlib import Path
import os,subprocess,argparse
parser=argparse.ArgumentParser();parser.add_argument("--start-at",type=int,default=0);args=parser.parse_args()
crate=Path(__file__).resolve().parents[1];source=crate/'src/portfolio_reads.rs'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','portfolio_reads','--jobs','1']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
for index,method in enumerate(['fetchBalance','fetchPositions','fetchOpenPositions','fetchOrders']):
 if index < args.start_at: continue
 original=source.read_bytes();text=original.decode();old='let Some(data) = truthy_input(data)? else {';assert text.count(old)==4
 pieces=text.split(old);mutated=pieces[0]
 for i,piece in enumerate(pieces[1:]):mutated+=('let Some(data) = data else {' if i==index else old)+piece
 try:
  source.write_text(mutated);r=subprocess.run(command,env=env,capture_output=True,text=True);out=r.stdout+r.stderr
  assert r.returncode==101 and f'portfolio_read_{method}:' in out and 'composed-falsy-' in out and 'test result: FAILED' in out,(method,r.returncode,out)
  print(f'{method}: compiled; exit101; portfolio_read_{method}: composed-falsy-0',flush=True)
 finally:source.write_bytes(original)
 r=subprocess.run(command,env=env,capture_output=True,text=True);assert r.returncode==0,(method,r.stdout+r.stderr)
 print(f'{method}: restored PASS',flush=True)
