#!/usr/bin/env python3
"""Require semantic native failures, restoring exact production bytes after each fault."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
env = {**os.environ, 'CARGO_INCREMENTAL': '0'}
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH', str(sdk.parent / 'node_modules/typescript'))
receipts = Path(tempfile.mkdtemp(prefix='kraken-retained-boundary-controls-'))
rows = []
cases = [
    ('helpers.rs',
     'error = error.with_retained_response(response.clone().into());',
     '''let mut response = response.clone();
        if let Some(fraction) = response.get_mut("fraction") { *fraction = serde_json::json!(0.0); }
        error = error.with_retained_response(response.into());''',
     ['--test', 'accepted_helpers', 'error_constructor_preserves_literal_message_distinct_snapshots_and_presence'],
     ['error_constructor_preserves_literal_message_distinct_snapshots_and_presence']),
    ('wire.rs',
     'Self::Text(v) => RetainedJsonInput::Text(v.encode_utf16().collect()),',
     'Self::Text(v) => RetainedJsonInput::Text(v.replace("\\u{fffd}", "x").encode_utf16().collect()),',
     ['--lib', 'accepted_boundary_observers'],
     ['binding::exchange::accepted_boundary_observers::token_and_leverage_bindings_project_only_the_closed_output',
      'binding::exchange::accepted_boundary_observers::fill_and_income_bindings_keep_raw_units_before_typed_model_projection']),
]
for filename, needle, replacement, selection, names in cases:
    path = sdk / 'safe-cex-venue-kraken/src' / filename
    original = path.read_bytes()
    text = original.decode()
    assert text.count(needle) == 1
    command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken', *selection, '--', '--nocapture']
    try:
        path.write_text(text.replace(needle, replacement))
        red = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{filename}-red.log').write_text(red.stdout)
        assert red.returncode != 0 and 'could not compile' not in red.stdout, red.stdout
        for name in names:
            assert f'test {name} ... FAILED' in red.stdout, red.stdout
        assert 'panicked at' in red.stdout, red.stdout
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    green = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (receipts / f'{filename}-restored.log').write_text(green.stdout)
    assert green.returncode == 0, green.stdout
    for name in names:
        assert f'test {name} ... ok' in green.stdout, green.stdout
    rows.append({'file': filename, 'names': names, 'red': red.returncode, 'restored': green.returncode})
    (receipts / 'receipts.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(f'{filename}: named compiled RED, exact restoration GREEN', flush=True)
print(receipts, flush=True)
