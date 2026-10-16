"""Five body-specific compiling signing-role controls; native observations stay unchanged."""
import os
from pathlib import Path
import subprocess
SDK = Path(__file__).resolve().parents[2]
SOURCE = SDK / 'safe-cex-venue-cryptocom/src/signing.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'set the live TypeScript compiler path'
CONTROLS = [
    ('params_to_string', 'signing', 'live_params', 'level >= 3.0', 'level > 3.0', 'params_native_value:array'),
    ('generate_signature', 'auth', 'live_auth', 'params_to_string(Some(params), None)', 'params_to_string({ let _ = params; None }, None)', 'auth_native_signature:0'),
    ('signature_from_params', 'auth', 'live_auth', 'hmac_sha256(api_secret.as_bytes(), payload.as_bytes(), Encoding::Hex)', 'hmac_sha256(api_secret.as_bytes(), payload.as_bytes(), Encoding::Hex) + "extra"', 'auth_native_signature:0'),
    ('generate_authenticated_request', 'auth', 'live_auth', 'nonce.unwrap_or_else(|| clock.get_current_time_ms())', 'nonce.unwrap_or(clock.get_current_time_ms())', 'AUTH-EAGER-CLOCK-COUNT'),
    ('generate_ws_auth_message', 'auth', 'live_auth', 'let nonce = clock.get_current_time_ms();', 'let nonce = id;', 'AUTH-EAGER-CLOCK-COUNT'),
]
def run(target, test):
    result = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-cryptocom', '--test', target, test, '--', '--exact'], cwd=SDK, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr
for owner, target, test, before, after, marker in CONTROLS:
    code, output = run(target, test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output
    original = SOURCE.read_bytes()
    start = original.index(f'fn {owner}('.encode())
    end = original.index(b'\n}\n', start) + 3
    region = original[start:end]
    assert region.count(before.encode()) == 1, owner
    try:
        SOURCE.write_bytes(original[:start] + region.replace(before.encode(), after.encode()) + original[end:])
        code, output = run(target, test)
        assert code == 101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output, output
        assert marker in output, output
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    code, output = run(target, test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output
    print(f'{owner}: compiled named native assertion {marker} failed (exit 101); exact restoration PASS', flush=True)
