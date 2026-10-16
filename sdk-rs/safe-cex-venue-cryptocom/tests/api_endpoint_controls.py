#!/usr/bin/env python3
"""One compiling substring-to-equality mutation for each newly mapped helper."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom/src/api.rs'
PUBLIC_SOURCE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom/src/public_endpoints.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_path_required'
COMMAND = ['cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-cryptocom', '--test', 'api_endpoints', '--jobs', '2', '--locked']


def run():
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    return result


def passed(result):
    assert result.returncode == 0 and '1 passed; 0 failed' in result.stdout, result.stdout


passed(run())
for name, file, before, after, assertion in [
    ('public_substring', PUBLIC_SOURCE, 'url.windows(endpoint.len()).any(|part| part == endpoint)', 'url == endpoint.as_slice()', 'endpoint_native_public:3'),
    ('private_write_substring', SOURCE, 'url.windows(endpoint.len()).any(|part| part == endpoint)', 'url == endpoint.as_slice()', 'endpoint_native_write:24'),
]:
    original = file.read_bytes()
    try:
        text = original.decode()
        assert text.count(before) == 1, 'endpoint_control_mutation_site'
        file.write_text(text.replace(before, after))
        result = run()
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and assertion in result.stdout, (name, result.stdout)
        print(f'FAIL observed {name}: {assertion}; exit=101', flush=True)
    finally:
        file.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
print('controls: 2 failures and restored passes', flush=True)
