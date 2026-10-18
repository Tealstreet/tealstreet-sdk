"""Serial compiling regression control; restores exact production bytes on failure."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / 'safe-cex-venue-woo/src/binding/hosts/order_mapping.rs'
original = source.read_bytes()
anchor = '    let reduce_only = v("reduce_only")?;'
closed_output = '    if kind.is_undefined() || price.is_undefined() {\n        return Err(Thrown::type_error("regression: closed order output"));\n    }\n'
logs = Path(tempfile.mkdtemp(prefix='woo-terminal-report-control-'))
env = {**os.environ, 'RUSTC_BOOTSTRAP': '1'}
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-woo', '--jobs=1', '--test', 'binding_streams']
try:
    text = original.decode()
    assert text.count(anchor) == 1, 'one live mapper boundary required'
    source.write_text(text.replace(anchor, closed_output + anchor))
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
print(f'Three real mapper closed-output regressions compile/fail; exact restoration passes all stream tests: {logs}')
