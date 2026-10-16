"""One row mutation and five production boundary controls, sequential whole-corpus fail/restore tests."""
import os
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/positions.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'positions', '--', '--nocapture']
MUTATIONS = {
    'BlofinExchange.mapPositions': ('multiply(pos, contract)', 'safe_cex_num::subtract(pos, contract)', 'positions_native_fields'),
}
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
rows = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.exchange.ts.toml').read_text())['symbol']
assert {r['ts'] for r in rows if r['status'] != 'pending' and any('/positions.rs::' in target for target in (r['rs'] if isinstance(r['rs'], list) else [r['rs']]))} == set(MUTATIONS), 'positions_row_mutation_census'
MUTATIONS.update({
    'numeric_equality_control': ('js_value_as_f64(a.unwrap()) == js_value_as_f64(b.unwrap())', '{ let _ = js_value_as_f64(a.unwrap()); a == b }', 'positions_warning_trace'),
    'reference_equality_control': ('std::ptr::eq(a, b)', 'a == b', 'positions_warning_trace'),
    'warning_source_control': ('js_string(rows[0].get("instId"))?', 'js_string(supported[0].get("instId"))?', 'positions_warning_trace'),
    'emitter_control': ('.map_err(PositionMapError::Emitter)?;', '.map_err(PositionMapError::Emitter).unwrap_or(());', 'positions_emitter_identity'),
    'ingress_control': ('#[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]', '', 'positions_ingress_retained'),
})
original = SOURCE.read_text()
logs = ROOT / 'sdk-rs/target/blofin-positions-controls'
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
