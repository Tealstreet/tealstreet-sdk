#!/usr/bin/env python3
"""Each production depth ingress bridge must retain raw JSON overflow values."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
crate = sdk / 'safe-cex-venue-binance'
native = crate / 'src/native_host.rs'
inverse = crate / 'src/inverse/ws_public.rs'
originals = {p: p.read_bytes() for p in [native, inverse]}
logs = Path(tempfile.mkdtemp(prefix='binance-depth-overflow-controls-'))
native_names = [
    'public_listener_aliases::production_native_depth_overflow_previous_id_keeps_contiguous_updates',
    'public_listener_aliases::production_native_depth_overflow_final_id_requests_resnapshot',
]
inverse_names = ['production_inverse_depth_overflow_ids_keep_native_sequence_rules']
changes = [
    ('native-apply', native,
     b'''safe_cex_exchange::js_runtime::value::to_retained_input(&data)
            .map_err(|error| exchange_failure(error.to_exchange_error()))?
            .into_value()
            .map_err(|error| failure(error.to_string()))?''',
     b'to_json(&data)?', 'private_classes', 'production_native_depth_overflow',
     native_names, 'source keeps contiguous raw overflow-pu update'),
    ('inverse-apply', inverse,
     b'''to_retained_input(update)?
            .into_value()
            .map_err(|error| Thrown::type_error(&error.to_string()))?''',
     b'safe_cex_exchange::js_runtime::value::to_json(update)?',
     'inverse_socket_binding', inverse_names[0], inverse_names,
     'buffered=false, previous=1e400'),
    ('inverse-buffer', inverse,
     b'''to_retained_input(v)?
                            .into_value()
                            .map_err(|error| Thrown::type_error(&error.to_string()))?''',
     b'safe_cex_exchange::js_runtime::value::to_json(v)?',
     'inverse_socket_binding', inverse_names[0], inverse_names,
     'buffered=true, previous=1e400'),
]


def run(label, target, selected):
    result = subprocess.run(
        ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-binance',
         '--features', 'native', '--test', target, selected], cwd=sdk,
        env={**os.environ, 'CARGO_BUILD_JOBS': '1'}, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    (logs / (label + '.log')).write_text(result.stdout)
    return result


def green(result, names):
    assert result.returncode == 0, result.stdout
    for name in names:
        assert 'test ' + name + ' ... ok' in result.stdout, result.stdout


receipts = []
try:
    for label, path, before, after, target, selected, names, assertion in changes:
        original = originals[path]
        assert path.read_bytes() == original and original.count(before) == 1
        green(run(label + '-baseline', target, selected), names)
        path.write_bytes(original.replace(before, after))
        try:
            result = run(label + '-red', target, selected)
            assert result.returncode != 0 and assertion in result.stdout, result.stdout
            assert 'could not compile' not in result.stdout and 'error[E' not in result.stdout, result.stdout
            for name in names:
                assert 'test ' + name + ' ... FAILED' in result.stdout, result.stdout
        finally:
            path.write_bytes(original)
        green(run(label + '-restored', target, selected), names)
        assert path.read_bytes() == original
        receipts.append({'seam': label, 'namedFailures': names,
                         'compiledFailure': True, 'exactRestorationPass': True})
finally:
    for path, original in originals.items():
        path.write_bytes(original)
print(json.dumps({'logs': str(logs), 'receipts': receipts,
                  'sourceSha256': {str(p.relative_to(sdk)): hashlib.sha256(v).hexdigest()
                                   for p, v in originals.items()}}, indent=2))
