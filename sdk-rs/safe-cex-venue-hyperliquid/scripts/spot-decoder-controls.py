"""Four compiled visitor faults; never count a compiler error as a control."""
import os
from pathlib import Path
import re
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-hyperliquid/src/spot_models.rs'
runner = sdk / 'safe-cex-venue-hyperliquid/scripts/spot-decoder-contracts.mjs'
original = source.read_bytes()
receipt_dir = Path(tempfile.mkdtemp(prefix='hl-spot-decoder-controls-'))
env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '1'}
faults = [
    ('WsFill', 'std::mem::swap(&mut decoded.px, &mut decoded.sz);'),
    ('WsUserFunding', 'std::mem::swap(&mut decoded.usdc, &mut decoded.szi);'),
    ('WsLiquidation', 'std::mem::swap(&mut decoded.liquidator, &mut decoded.liquidated_user);'),
    ('WsNonUserCancel', 'decoded.oid = decoded.oid.abs();'),
]


def run(label):
    log = receipt_dir / (label + '.log')
    with log.open('w') as output:
        result = subprocess.run(['node', str(runner)], cwd=sdk, env=env,
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, log.read_text()


code, output = run('baseline')
assert code == 0, output[-12000:]
for name, fault in faults:
    before = f'{name}Wire::deserialize(serde::de::value::MapAccessDeserializer::new(map))'
    after = before + f'.map(|mut decoded| {{ {fault} decoded }})'
    text = original.decode()
    assert text.count(before) == 1, name
    try:
        source.write_text(text.replace(before, after))
        code, output = run(name + '-red')
        assert code != 0, f'{name}: mutation unexpectedly passed'
        assert not re.search(r'error\[E\d+\]|could not compile|did not complete', output), output[-12000:]
        assert f'SPOT_DECODER_NATIVE safe_cex_venue_hyperliquid::spot_models::{name}' in output, output[-12000:]
        assert 'native observation mismatch' in output, output[-12000:]
        print(f'{name}: named compiled native observation RED', flush=True)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    code, output = run(name + '-restored')
    assert code == 0, output[-12000:]
    print(f'{name}: exact-source restoration GREEN', flush=True)
print(f'4 compiled visitor faults and restorations PASS: {receipt_dir}', flush=True)
