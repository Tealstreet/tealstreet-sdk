"""Independent production controls; sequential one-target tests."""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitunix/src/order_ids.rs'
original = source.read_text()
logs = root / 'sdk-rs/target/bitunix-order-id-controls'
logs.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitunix', '--jobs', '1', '--test', 'order_ids', '--', '--nocapture']
for old,new,name in [('ends_with("__tp")','ends_with("__zz")','virtual leg'),('(2 + leg.len())','leg.len()','virtual parent')]:
    assert old in original
    try:
        source.write_text(original.replace(old,new))
        result = subprocess.run(command, text=True, capture_output=True)
        (logs/(name.replace(' ','-')+'-fail.log')).write_text(result.stdout+result.stderr)
        assert result.returncode not in (0,126) and name in result.stdout+result.stderr, result.stdout+result.stderr
        print('FAIL observed: '+name, flush=True)
    finally:
        source.write_text(original)
    result = subprocess.run(command, text=True, capture_output=True)
    (logs/(name.replace(' ','-')+'-pass.log')).write_text(result.stdout+result.stderr)
    assert result.returncode == 0, result.stdout+result.stderr
    print('PASS restored: '+name, flush=True)
