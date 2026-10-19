"""Compiled native controls for the real order-error constructor/parser/consumer."""
import os
from pathlib import Path
import subprocess
import tempfile

package = Path(__file__).resolve().parents[1]
workspace = package.parent
imports = package / 'src/realm/imports_venue.rs'
consumer = package / 'src/order_request_leaves.rs'
originals = {p: p.read_bytes() for p in [imports, consumer]}
logs = Path(tempfile.mkdtemp(prefix='hl-order-error-controls-'))
env = {**os.environ, 'CARGO_INCREMENTAL': '0'}
command = ['cargo', 'test', '--locked', '--jobs', '1', '-p',
           'safe-cex-venue-hyperliquid', '--lib', 'binding::order_error_audit::']


def run(label):
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / f'{label}.log').write_text(output)
    return result.returncode, output


print(f'logs: {logs}', flush=True)
status, output = run('baseline')
assert status == 0 and '2 passed; 0 failed' in output, output
faults = [
    ('constructor-name', imports,
     'Value::str("HyperliquidUnreadableOrderResponseError"),',
     'Value::str("Error"),', 'order_error_audit_bound_constructor_retains_error_inheritance_and_units'),
    ('parser-inheritance', imports,
     'Err(r.construct(&class, &[Value::str(&e.message)], None)?)',
     'let _ = class; Err(r.error(&e.message))',
     'order_error_audit_real_response_consumer_reconciles_only_unreadable_errors'),
    ('reconcile-guard', consumer,
     'if h.instance_of(&error, &class)? {',
     'if !h.instance_of(&error, &class)? {',
     'order_error_audit_real_response_consumer_reconciles_only_unreadable_errors'),
]
try:
    for name, path, before, after, test in faults:
        text = originals[path].decode()
        assert text.count(before) == 1, (name, before)
        path.write_text(text.replace(before, after))
        status, output = run(f'{name}-red')
        assert status == 101 and 'error[E' not in output, output
        assert f'{test} ... FAILED' in output and 'HL_ORDER_ERROR' in output, output
        print(f'{name}: compiled named native RED', flush=True)
        path.write_bytes(originals[path])
        status, output = run(f'{name}-restored')
        assert status == 0 and '2 passed; 0 failed' in output, output
        assert path.read_bytes() == originals[path]
        print(f'{name}: exact restoration GREEN', flush=True)
finally:
    for path, original in originals.items():
        path.write_bytes(original)
print('3 compiled native controls/exact restorations PASS', flush=True)
