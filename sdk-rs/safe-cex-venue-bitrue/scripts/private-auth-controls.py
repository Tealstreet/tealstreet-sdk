#!/usr/bin/env python3
"""Eight disjoint private-auth defects, named native assertions, exact restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
p=crate/'src/private_auth.rs';saved=p.read_bytes();source=saved.decode()
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
controls=[
 ('create_api','h.number(3.0)','h.number(4.0)','api_native','api/signed'),
 ('construct','"listenKeyRefreshTimeoutId", h.undefined()','"listenKeyRefreshTimeoutId", h.number(0.0)','class_native','class/pair.timeout'),
 ('incremental_fills','h.create_incremental()','h.create_incremental().map(|v| h.array(vec![v]))','incremental_native','incremental/factory-result'),
 ('connect','h.text("?listenKey=")','h.text("?wrongKey=")','connect_native','connect/subscribe'),
 ('message','"trackLastReceived"','"recordMessageHandlerStats"','message_native','message/order'),
 ('order_update','Op::Mul, filled_raw.clone(), multiplier.clone()','Op::Add, filled_raw.clone(), multiplier.clone()','order_native','order/incremental-raw-scaling'),
 ('fetch_key','h.text("warning")','h.text("error")','fetch_native','fetch/retry-success'),
 ('update_key','Failed to refresh Bitrue futures listen key; private updates may stop when it expires.','Wrong refresh reason.','refresh_native','refresh/invalid-response'),
]
for family,old,new,test,assertion in controls:
 marker=('pub async fn ' if family in ['connect','fetch_key','update_key'] else 'pub fn ')+family+'<'
 start=source.index(marker);ends=[i for token in ['\npub fn ','\npub async fn ','\npub const '] if (i:=source.find(token,start+len(marker)))>=0];end=min(ends) if ends else len(source)
 part=source[start:end];assert part.count(old)==1,(family,old,part.count(old));source=source[:start]+part.replace(old,new,1)+source[end:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','private_auth','--jobs','2','--locked'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run();assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for family,old,new,test,assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
  assert 'native' in failure and assertion in failure,(test,failure)
  print(f'{test}: compiled native {assertion} assertion exit101',flush=True)
finally:p.write_bytes(saved)
restored=run();assert restored.returncode==0,restored.stdout
for family,old,new,test,assertion in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit0',flush=True)
assert p.read_bytes()==saved
print(f'{revision}: eight compiled defects; eight named native failures/restored passes; exact bytes restored')
