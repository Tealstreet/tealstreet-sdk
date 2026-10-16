#!/usr/bin/env python3
"""Nine disjoint trading-family defects with named native failures/restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/trading_rest.rs';saved=p.read_bytes();source=saved.decode()
controls=[
 ('markets','h.number(100.0)','h.number(101.0)','markets_native','markets/raw-market'),
 ('candles','h.number(10_000.0)','h.number(10_001.0)','candles_native','candles/request-cache'),
 ('format_order','h.text("normal")','h.text("wrong-model")','format_native','format/limit'),
 ('place','Ok(res)','Ok(h.array(vec![res]))','place_native','place/original-result'),
 ('places','h.text("fetchAndSetOrders")','h.text("wrong-worker-method")','places_native','places/flat-results'),
 ('batch','h.chunk(payloads, 5)?','h.chunk(payloads, 4)?','batch_native','batch/chunks-completion-order'),
 ('update','for k in ["price", "amount"]','for k in ["wrongPrice", "amount"]','update_native','update/explicit-zero'),
 ('leverage','"delete",\n            vec![symbol.clone()]','"delete",\n            vec![h.text("wrong-symbol")]','leverage_native','leverage/delete-cache'),
 ('cancel','h.chunk(payloads, 50)?','h.chunk(payloads, 49)?','cancel_native','cancel/chunk-50'),
]
for name,old,new,test,assertion in controls:
 marker=('pub fn ' if name=='format_order' else 'pub async fn ')+name+'<'
 start=source.index(marker);candidates=[i for t in ['\npub fn ','\npub async fn '] if (i:=source.find(t,start+len(marker)))>=0];end=min(candidates) if candidates else len(source)
 part=source[start:end];assert part.count(old)==1,(test,old,part.count(old));source=source[:start]+part.replace(old,new,1)+source[end:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','trading_rest','--jobs','2','--locked'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for name,old,new,test,assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
  assert 'native' in failure and assertion in failure,(test,failure)
  print(f'{test}: compiled native {assertion} assertion exit101',flush=True)
finally:p.write_bytes(saved)
restored=run();assert restored.returncode==0,restored.stdout
for name,old,new,test,assertion in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit0',flush=True)
assert p.read_bytes()==saved
print(f'{revision}: nine compiled defects; nine named native failures/restored passes; exact bytes restored')
