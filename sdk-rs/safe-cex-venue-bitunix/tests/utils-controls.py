"""One production control, one Node process per full corpus, sequential Cargo."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitunix/src/utils.rs'
original = source.read_text()
changed = original.replace('{byte:02x}', '{byte:02X}')
assert changed != original
logs = root / 'sdk-rs/target/bitunix-utility-controls'
logs.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitunix', '--jobs', '1', '--test', 'utils', '--', '--nocapture']
try:
    source.write_text(changed)
    result = subprocess.run(command, text=True, capture_output=True)
    (logs / 'fail.log').write_text(result.stdout + result.stderr)
    assert result.returncode not in (0, 126) and 'utility sha256Hex' in result.stdout + result.stderr, result.stdout + result.stderr
    print('FAIL observed: utility sha256Hex', flush=True)
finally:
    source.write_text(original)
result = subprocess.run(command, text=True, capture_output=True)
(logs / 'pass.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('PASS restored: utility sha256Hex', flush=True)
