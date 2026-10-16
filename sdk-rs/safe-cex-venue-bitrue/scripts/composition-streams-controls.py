#!/usr/bin/env python3
"""Six class/constructor/stream defects must fail named native assertions."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
paths=[crate/'src/composition.rs',crate/'src/public_streams.rs'];saved={p:p.read_bytes() for p in paths};sources={p:p.read_text() for p in paths}
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
controls=[
 (paths[0],'initialize_exchange','"inProgressOrderFetchId", h.number(0.0)','"inProgressOrderFetchId", h.number(1.0)','exchange_class_native','exchange_class/pair.initial-id'),
 (paths[0],'constructor_body','host.number(5.0)','host.number(4.0)','constructor_native','constructor/wiring'),
 (paths[0],'construct_public','"topics", h.object(vec![])','"topics", h.object(vec![("wrong", h.boolean(true))])','public_class_native','public_class/pair'),
 (paths[1],'message','"trackLastReceived"','"cleanupMessageHandlerStats"','message_native','message/ping-zero'),
 (paths[1],'listen_candles','host.number(100.0)','host.number(101.0)','candles_native','candles/waiting'),
 (paths[1],'listen_book','host.number(3000.0)','host.number(3001.0)','book_native','book/primary-update'),
]
for p,family,old,new,test,label in controls:
 source=sources[p];marker=('pub async fn ' if family=='message' else 'pub fn ')+family+'<';start=source.index(marker);ends=[i for t in ['\npub fn ','\npub async fn ','\nfn ','\nstruct '] if (i:=source.find(t,start+len(marker)))>=0];end=min(ends) if ends else len(source);part=source[start:end];assert part.count(old)==1,(family,old,part.count(old));sources[p]=source[:start]+part.replace(old,new,1)+source[end:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','composition_streams','--jobs','2','--locked'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 for p in paths:p.write_text(sources[p])
 broken=run();assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for p,family,old,new,test,label in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0];assert 'native' in failure and label in failure,(test,failure)
  print(f'{test}: compiled native {label} assertion exit101',flush=True)
finally:
 for p in paths:p.write_bytes(saved[p])
restored=run();assert restored.returncode==0,restored.stdout
for p,family,old,new,test,label in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit0',flush=True)
for p in paths:assert p.read_bytes()==saved[p]
print(f'{revision}: six compiled defects; six named native failures/restored passes; exact bytes restored')
