"""Three production row mutations, sequential whole-corpus fail/restore tests."""
import os
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/order_history.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'history']
MUTATIONS = {
    'toOptionalNumber': ('parsed != 0.0', 'parsed != 1.0', 'history_optional_native'),
    'toNumber': ('Ok(if parsed.is_finite()', 'Ok(if !parsed.is_nan()', 'history_to_number_native'),
    'mapBlofinHistoricalOrder': ('("market", OrderType::Market)', '("market", OrderType::Limit)', 'history_order_native'),
}
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
rows = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.order-history.ts.toml').read_text())['symbol']
assert {r['ts'] for r in rows if r['status'] != 'pending'} == set(MUTATIONS), 'history_row_mutation_census'
MUTATIONS['inherited_control'] = (
    '\n'.join(['TableHit::Inherited(_) => Err(JsCoercionError {', '            name: "RepresentationError",', '            message: "Inherited history table value is outside the shared order enum",', '        }),']),
    'TableHit::Inherited(_) => Ok(fallback),', 'history_inherited_refusal')
original = SOURCE.read_text()
logs = ROOT / 'sdk-rs/target/blofin-history-controls'
logs.mkdir(exist_ok=True)
for name, (old, new, assertion) in MUTATIONS.items():
    assert original.count(old) == 1, name + '_anchor'
    try:
        SOURCE.write_text(original.replace(old, new))
        result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / (name + '.fail.log')).write_text(result.stdout)
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print(f'{name}: FAIL {assertion} exit=101', flush=True)
    finally:
        SOURCE.write_text(original)
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.restored.log')).write_text(result.stdout)
    assert result.returncode == 0, result.stdout
    print(f'{name}: RESTORED exit=0', flush=True)
