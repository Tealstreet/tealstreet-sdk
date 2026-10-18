"""Compile a real mapper regression, require named native failures, restore exactly."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / 'safe-cex-venue-woo/src/binding/hosts/order_mapping.rs'
original = source.read_bytes()
anchor = '        let id = Js::utf16(&v("algoOrderId")?.to_js_string()?);'
closed = '''        if kind.is_undefined() {
            return Err(Thrown::type_error("regression: closed algo type"));
        }
'''
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-woo', '--jobs=1', '--lib', 'binding::exchange::live_order_mapping_tests']
env = {**os.environ, 'RUSTC_BOOTSTRAP': '1'}
logs = Path(tempfile.mkdtemp(prefix='woo-algo-order-control-'))
try:
    text = original.decode()
    assert text.count(anchor) == 1
    source.write_text(text.replace(anchor, closed + anchor))
    run = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
    output = run.stdout + run.stderr
    (logs / 'closed-algo-type.log').write_text(output)
    assert run.returncode == 101 and 'test result: FAILED' in output, output
    assert 'could not compile' not in output and 'error[E' not in output, output
    for name in ['live_algo_mapper_preserves_partial_stop_and_copied_fields', 'live_algo_fetch_keeps_partial_stop_and_both_bracket_children']:
        assert f'{name} ... FAILED' in output, output
    assert 'one partial STOP must not discard the complete algo-order set' in output, output
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
(logs / 'restored.log').write_text(restored.stdout + restored.stderr)
assert restored.returncode == 0, restored.stdout + restored.stderr
assert source.read_bytes() == original
print(f'Real mapper and HTTP regressions compile and fail; restored native mapper suite passes: {logs}')
