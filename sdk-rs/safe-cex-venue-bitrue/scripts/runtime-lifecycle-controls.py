#!/usr/bin/env python3
"""Nine runtime defects must reach named native failures and exact restored passes."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
p=crate/'src/runtime_lifecycle.rs';saved=p.read_bytes();source=saved.decode()
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
controls=[
 ('dispose','h.text("fill")','h.text("wrong-fill")','dispose_native','dispose/active'),
 ('queue_refresh','h.number(350.0)','h.number(351.0)','queue_native','queue/debounce'),
 ('run_refresh','h.boolean(false)','h.boolean(true)','refresh_native','refresh/success'),
 ('validate','Invalid API Key or Secret','Wrong validation fallback','validate_native','validate/invalid-fallback'),
 ('start','("bid", h.number(0.0))','("bid", h.number(1.0))','start_native','start/authenticated'),
 ('tick','get(h, &p, "contracts")','get(h, &p, "wrongContracts")','tick_native','tick/closed-positions'),
 ('parse_positions','h.less(h.number(0.0), v)?','h.less(h.number(100.0), v)?','positions_native','positions/mapped'),
 ('listen_candles','get(h, &opts, "symbol")','get(h, &opts, "interval")','candles_native','candles/original-return'),
 ('listen_book','call(h,','call(h,','book_native','book/original-return'),
]
for family,old,new,test,assertion in controls:
 marker=('pub fn ' if family in ['dispose','queue_refresh','listen_candles','listen_book'] else 'pub async fn ')+family+'<'
 start=source.index(marker);ends=[i for t in ['\npub fn ','\npub async fn ','\nfn '] if (i:=source.find(t,start+len(marker)))>=0];end=min(ends) if ends else len(source);part=source[start:end]
 if family=='listen_book':
  old='    call(';assert part.count(old)==1;part=part.replace(old,'    let result = call(',1);part=part.rsplit('\n}',1)[0].rstrip()+'?;\n    Ok(h.array(vec![result]))\n}\n'
 else:
  assert part.count(old)==1,(test,old,part.count(old));part=part.replace(old,new,1)
 source=source[:start]+part+source[end:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','runtime_lifecycle','--jobs','2','--locked'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run();assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for family,old,new,test,assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0];assert 'native' in failure and assertion in failure,(test,failure)
  print(f'{test}: compiled native {assertion} assertion exit101',flush=True)
finally:p.write_bytes(saved)
restored=run();assert restored.returncode==0,restored.stdout
for family,old,new,test,assertion in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit0',flush=True)
assert p.read_bytes()==saved
print(f'{revision}: nine compiled defects; nine named native failures/restored passes; exact bytes restored')
