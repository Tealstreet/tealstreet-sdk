"""Serial compiling regression control; restores exact production bytes on failure."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / 'safe-cex-venue-woo/src/binding/hosts/members.rs'
original = source.read_bytes()
anchor = '|rt, a| super::order_mapping::map_limit_order(rt, arg(&a, 0))'
old_typed_boundary = '''|rt, a| {
    let _live_mapper = super::order_mapping::map_limit_order;
    let store = rt.shared.store.clone();
    match orders::map_limit_order(&object(&arg(&a, 0))?, store.as_ref(), || rt.now())
        .map_err(|e| Thrown::type_error(&e))? {
        Some(order) => from_typed(&order), None => Ok(Js::Null),
    }
}'''
logs = Path(tempfile.mkdtemp(prefix='woo-terminal-report-control-'))
env = {**os.environ, 'RUSTC_BOOTSTRAP': '1'}
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-woo', '--jobs=1', '--test', 'binding_streams']
try:
    text = original.decode()
    assert text.count(anchor) == 1, 'one live mapper binding required'
    source.write_text(text.replace(anchor, old_typed_boundary))
    result = subprocess.run(command + ['execution_report_emits_fill_and_closes_order_through_real_mapper'], cwd=root, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / 'typed-boundary.log').write_text(output)
    assert result.returncode == 101 and 'running 3 tests' in output and 'could not compile' not in output, output
    assert '3 failed' in output and output.count('terminal report must emit fill through the real mapper') == 3, output
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
(logs / 'restored.log').write_text(restored.stdout + restored.stderr)
assert restored.returncode == 0, restored.stdout + restored.stderr
assert source.read_bytes() == original
print(f'Three real mapper regressions compile/fail; exact restoration passes all stream tests: {logs}')
