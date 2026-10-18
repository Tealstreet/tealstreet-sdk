#!/usr/bin/env python3
"""Verify the rejection witness distinguishes HTTP rejection from bad fulfilled data.

This mutates only the terminal test fixture, independently of production controls.
"""
from pathlib import Path
import subprocess,re,json,hashlib
sdk=Path(__file__).resolve().parents[2]
source=sdk/'safe-cex-venue-binance/tests/support/snapshot_future_binding.rs'
original=source.read_bytes()
logs=sdk/'target/snapshot-rejection-fixture-control'
logs.mkdir(parents=True,exist_ok=True)
name='snapshot_future_binding::snapshot_futures_propagate_conditional_rejection_to_source_fallback'
cmd=['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-binance','--features','native','--test','private_classes',name,'--','--exact']
def run(label):
    p=logs/(label+'.log')
    with p.open('w') as f:
        r=subprocess.run(cmd,cwd=sdk,stdout=f,stderr=subprocess.STDOUT)
    return r.returncode,p.read_text()
status,log=run('real-http-rejection-green')
assert status==0 and '1 passed; 0 failed' in log,log[-3000:]
s=original.decode(); start=s.index('Poll::Ready(if response.status == 200 {');end=s.index('\n            })',start)+len('\n            })')
try:
    source.write_text(s[:start]+'Poll::Ready(Ok(response))'+s[end:])
    status,log=run('old-fulfilled-400-red')
    assert status!=0 and f'test {name} ... FAILED' in log
    assert not re.search(r'^error(?:\[|: (?!test failed\b))',log,re.M),log[-3000:]
    assert 'source catch forwards rejected response.data.msg through Base formatting' in log
finally:
    source.write_bytes(original)
    assert source.read_bytes()==original
status,log=run('exact-restoration-green')
assert status==0 and '1 passed; 0 failed' in log,log[-3000:]
(logs/'results.json').write_text(json.dumps({'kind':'fixture-semantics-control-not-production-mutation','test':name,'fixtureSha256':hashlib.sha256(original).hexdigest(),'compiledOldFixtureRed':True,'exactRestorationGreen':True},indent=2)+'\n')
print('Exact error assertion rejects old fulfilled-400 fixture after compilation; genuine HTTP rejection restored GREEN.',flush=True)
