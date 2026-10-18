#!/usr/bin/env python3
"""Remove only production Exchange cancel-all dispatch and require four named failures."""
import hashlib,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
source=root/'safe-cex-venue-bitget/src/binding/exchange.rs'
original=source.read_bytes()
block=b'''    // ts: BitgetExchange.cancelAllOrders / BitgetInverseExchange.cancelAllOrders
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelAllOrders", vec![])
    }
'''
assert original.count(block)==1
labels=['real classic cancel-all must call venue with empty Store','real UTA cancel-all must fetch venue normal orders','real inverse cancel-all must preserve unique market coin order','real cancel-all failure must attempt venue cancellation']
def run():
    return subprocess.run(['cargo','test','--locked','-p','safe-cex-venue-bitget','--jobs=1','--test','cancellation_binding','--','--nocapture'],cwd=root,env={**os.environ,'RUSTC_BOOTSTRAP':'1'},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
baseline=run();assert baseline.returncode==0,baseline.stdout
print('BASELINE real classic/UTA/inverse/failure4tests PASS',flush=True)
try:
    source.write_bytes(original.replace(block,b''))
    result=run()
    assert result.returncode==101 and '0 passed; 4 failed' in result.stdout and all(label in result.stdout for label in labels),result.stdout
    print(result.stdout,flush=True)
finally:
    source.write_bytes(original)
    assert source.read_bytes()==original
restored=run();assert restored.returncode==0,restored.stdout
print('CONTROL trait-cancel-all-dispatch:4compiled named RED / exact-restored4PASS; SHA256 '+hashlib.sha256(original).hexdigest(),flush=True)
