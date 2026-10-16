"""One compiling native-result control per newly ported normalizer; run serially."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
source = crate / 'src/market_data.rs'
original = source.read_bytes()
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
           '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'market_data']
controls = [
    (b'amount: amount_step,\n            price: price_step,', b'amount: price_step,\n            price: amount_step,', 'market_data::market::distinct::route_0::amount'),
    (b'volume: to_number(pick(&["volume", "v"]).as_ref(), None)?',
     b'volume: to_number(pick(&["quoteVolume", "q"]).as_ref(), None)?',
     'market_data::ticker::distinct::route_0::volume'),
]
try:
    for before, after, label in controls:
        assert original.count(before) == 1, before
        source.write_bytes(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output, output
        print(f'COMPILED; exit101; {label}', flush=True)
        source.write_bytes(original)
finally:
    source.write_bytes(original)
subprocess.run(command, check=True)
print('RESTORED exit0; 2 new-row production controls', flush=True)
