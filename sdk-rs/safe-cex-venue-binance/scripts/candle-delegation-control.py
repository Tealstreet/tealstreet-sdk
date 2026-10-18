#!/usr/bin/env python3
"""Prove the real native candle witness catches a lost public-to-market delegation."""
from pathlib import Path
import hashlib
import json
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/public_classes.rs'
original = source.read_bytes()
logs = crate.parent / 'target/candle-delegation-control'
logs.mkdir(parents=True, exist_ok=True)
name = 'production_native_candle_delegation_uses_market_socket_and_cleanup'
command = ['cargo', 'test', '--color', 'never', '--locked', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--jobs', '1', '--test', 'private_classes', name, '--', '--exact']


def run(label):
    path = logs / (label + '.log')
    with path.open('w') as output:
        result = subprocess.run(command, cwd=crate.parent, stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


status, log = run('baseline-green')
assert status == 0 and '1 passed; 0 failed' in log
before = '''            PublicKind::FuturesPublic | PublicKind::PMPublic => self
                .market_websocket
                .get()
                .unwrap()
                .listen_ohlcv(opts, callback),'''
after = '''            PublicKind::FuturesPublic | PublicKind::PMPublic => {
                let _ = (opts, callback);
                Ok(Rc::new(|| Ok(())))
            }'''
assert original.decode().count(before) == 1
try:
    source.write_text(original.decode().replace(before, after))
    status, log = run('lost-delegation-red')
    assert status != 0 and not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M)
    assert f'test {name} ... FAILED' in log and '0 passed; 1 failed' in log
finally:
    source.write_bytes(original)
assert source.read_bytes() == original
status, log = run('exact-restoration-green')
assert status == 0 and '1 passed; 0 failed' in log
(logs / 'result.json').write_text(json.dumps({'test': name,
    'sourceSha256': hashlib.sha256(original).hexdigest(),
    'namedCompiledFailure': True, 'exactRestorationPass': True}, indent=2) + '\n')
print('Lost live candle delegation: compiled named RED; exact restoration GREEN.')
