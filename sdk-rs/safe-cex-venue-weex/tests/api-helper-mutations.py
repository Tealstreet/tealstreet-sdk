"""One compiling control per new API row, with individual restoration checks."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
controls = [
    ('signing.rs', b'mac.update(text.as_bytes());', b'mac.update(text.as_bytes());\n    mac.update(key.as_bytes());', 'api_helpers::signature::ascii'),
    ('retry.rs', b'retries: 3,', b'retries: 4,', 'api_helpers::config::retries'),
    ('retry.rs', b'    should_retry_weex_request(method, url, response_status, retry_count, is_network_error)', b'    !should_retry_weex_request(method, url, response_status, retry_count, is_network_error)', 'api_helpers::condition::network_get'),
    ('retry.rs', b'        rate_limit_delay()', b'        rate_limit_delay() + 1.0', 'api_helpers::delay::rate'),
]
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'api_helpers']
for filename, before, after, label in controls:
    path = crate / 'src' / filename
    original = path.read_bytes()
    try:
        assert original.count(before) == 1, before
        path.write_bytes(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output, output
        print(f'COMPILED exit101 {label}', flush=True)
    finally:
        path.write_bytes(original)
    subprocess.run(command, check=True)
    print(f'RESTORED exit0 {label}', flush=True)
print('4 new-row controls caught and individually restored', flush=True)
