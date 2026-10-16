"""One source corpus per test run; serial production controls with exact restoration."""
from pathlib import Path
import subprocess
import os
crate = Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-orderly', '--test', 'position_side_parity', '--jobs', '1', '--locked']
def run():
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
controls = [
    ('position_mode == PositionMode::Hedged', 'position_mode == PositionMode::OneWay', 'POSITION-SIDE native/hedged/buy/undefined'),
    ('if reduce_only.unwrap_or(false)', 'if !reduce_only.unwrap_or(false)', 'POSITION-SIDE native/hedged/buy/undefined'),
]
baseline = run()
assert baseline.returncode == 0, baseline.stdout
for old, new, assertion in controls:
    file = crate / 'src/utils.rs'
    saved = file.read_bytes()
    try:
        source = saved.decode()
        assert source.count(old) == 1, old
        file.write_text(source.replace(old, new))
        result = run()
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print('compiled exit 101: ' + assertion, flush=True)
    finally:
        file.write_bytes(saved)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    print('restored exit 0: ' + assertion, flush=True)
