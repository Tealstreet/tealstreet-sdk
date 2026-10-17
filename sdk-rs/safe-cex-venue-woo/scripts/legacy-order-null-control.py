"""Compile a null-refusal in the live mapper; restore exact production bytes."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / 'safe-cex-venue-woo/src/binding/hosts/order_mapping.rs'
original = source.read_bytes()
anchor = '    let order_id = v("order_id")?;'
refusal = ('    if row.get("amount")?.is_nullish() {\n'
           '        return Err(Thrown::type_error("regression: legacy amount must be a number"));\n'
           '    }\n')
logs = Path(tempfile.mkdtemp(prefix='woo-legacy-null-control-'))
env = {**os.environ, 'RUSTC_BOOTSTRAP': '1'}
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-woo', '--jobs=1',
           '--test', 'binding_orders']
try:
    text = original.decode()
    assert text.count(anchor) == 1, 'one production mapper required'
    source.write_text(text.replace(anchor, refusal + anchor))
    result = subprocess.run(command + [
        'fetch_orders_accepts_documented_null_legacy_metadata_through_the_binding',
        '--', '--exact'], cwd=root, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / 'refusal.log').write_text(output)
    assert result.returncode == 101 and 'could not compile' not in output, output
    assert 'documented nullable metadata must not drop the order' in output, output
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
(logs / 'restored.log').write_text(restored.stdout + restored.stderr)
assert restored.returncode == 0, restored.stdout + restored.stderr
assert source.read_bytes() == original
print(f'Production null-refusal compiles/fails; exact restoration passes order binding tests: {logs}')
