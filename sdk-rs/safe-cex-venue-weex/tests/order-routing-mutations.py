"""One compiling native-result mutation for each new order routing row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
path = crate / 'src/order_routing.rs'
original = path.read_bytes()
controls = [
    (b'Some(OrderType::Limit | OrderType::Market)', b'Some(OrderType::Limit | OrderType::Market | OrderType::Conditional)', 'routing::batch::base_Conditional'),
    (b'pair == [u16::from(b\'_\'), u16::from(b\'_\')]', b'pair == [u16::from(b\'_\'), u16::from(b\'-\')]', 'routing::algo::Limit_double'),
    (b'            PositionSide::Short', b'            PositionSide::Long', 'routing::position::Buy_true'),
]
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'order_routing']
try:
    for before, after, label in controls:
        assert original.count(before) == 1, before
        path.write_bytes(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output, output
        print(f'COMPILED exit101 {label}', flush=True)
        path.write_bytes(original)
        subprocess.run(command, check=True)
        print(f'RESTORED exit0 {label}', flush=True)
finally:
    path.write_bytes(original)
print('3 new-row controls caught and individually restored', flush=True)
