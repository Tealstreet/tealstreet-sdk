"""Audit each implemented market price-step read against the ORIGINAL position corpus."""
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/positions.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'positions', '--', '--nocapture']
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
original = SOURCE.read_text()
assert original.count('market.precision.price') == 2, 'price_step_site_census'
assert original.count('market.precision.amount') == 0, 'amount_step_site_census'
mutations = {
    'position_notional': ('market.precision.price,', 'market.precision.amount,'),
    'position_unrealized_pnl': ('market.precision.price)', 'market.precision.amount)'),
}
logs = ROOT / 'sdk-rs/target/blofin-precision-controls'
logs.mkdir(exist_ok=True)
for name, (old, new) in mutations.items():
    assert original.count(old) == 1, name + '_anchor'
    try:
        SOURCE.write_text(original.replace(old, new))
        result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / (name + '.fail.log')).write_text(result.stdout)
        assert result.returncode == 101 and 'panicked at' in result.stdout and 'positions_native_fields' in result.stdout, result.stdout
        print(f'{name}: FAIL positions_native_fields exit=101', flush=True)
    finally:
        SOURCE.write_text(original)
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.restored.log')).write_text(result.stdout)
    assert result.returncode == 0, result.stdout
    print(f'{name}: RESTORED exit=0', flush=True)
