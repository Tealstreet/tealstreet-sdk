#!/usr/bin/env python3
"""Compiled HTTP declaration/constructor boundary controls with native assertions."""
from pathlib import Path
import os
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-lighter-sdk', '--test', 'http_types', '--jobs', '1', '--locked']
callables = 'callable_declarations_keep_values_identity_and_optional_lists'
records = 'referral_records_retain_native_values_and_shared_nested_objects'
controls = [
    ('request', 'http_types.rs', 'self.request.as_ref()?.borrow().first().cloned()', 'self.request.as_ref()?.borrow().get(1).cloned()', callables, 'SDK-HTTP callback identity request 0'),
    ('response', 'http_types.rs', 'self.response.as_ref()?.borrow().first().cloned()', 'self.response.as_ref()?.borrow().get(1).cloned()', callables, 'SDK-HTTP callback identity response 0'),
    ('error', 'http_types.rs', 'self.error.as_ref()?.borrow().first().cloned()', 'self.error.as_ref()?.borrow().get(1).cloned()', callables, 'SDK-HTTP callback identity error 0'),
    ('configuration', 'http_types.rs', '            request,\n            response,\n            error,', '            request: response,\n            response: request,\n            error,', 'interceptor_configuration_preserves_optional_field_arrays', 'SDK-HTTP config count request 1'),
    ('api-error-class-constructor', 'http_types.rs', 'name: "LighterApiError",', 'name: "WrongApiError",', 'api_error_fields_and_original_details_match_actual_constructor', 'SDK-HTTP native error name'),
    ('points', 'referral_types.rs', '            total_points,', '            total_points: total_points + 1.0,', records, 'SDK-HTTP native number totalPoints'),
    ('response-record', 'referral_types.rs', '        Self { referral_points }', '        referral_points.borrow_mut().total_points += 1.0;\n        Self { referral_points }', records, 'SDK-HTTP native number response totalPoints'),
]
selected = set(sys.argv[1:])
assert selected <= {c[0] for c in controls}, 'unknown family'
for name, file, before, after, test, assertion in controls:
    if selected and name not in selected:
        continue
    source = crate / 'src' / file
    original = source.read_text()
    assert original.count(before) == 1, f'{name}: unique production operation'
    try:
        source.write_text(original.replace(before, after))
        broken = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (sdk / 'target' / f'lighter-sdk-http-types-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'{name}: {broken.stdout}'
        assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
        print(f'{name}: compiled 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'lighter-sdk-http-types-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration {restored.stdout}'
    assert source.read_text() == original, 'production bytes restored'
    print(f'{name}: restored 0', flush=True)
