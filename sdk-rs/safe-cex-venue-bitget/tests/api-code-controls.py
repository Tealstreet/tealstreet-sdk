#!/usr/bin/env python3
"""Run alone: restore the original buffered union and require its native failure."""
import os
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[2]
source = root / 'safe-cex-venue-bitget/src/account_predicates.rs'
original = source.read_bytes()
text = original.decode()
start = text.index('#[derive(Debug)]\npub enum ApiCode')
end = text.index('// ts: isBitgetUtaClassicUnsupported')
buffered = '''#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ApiCode {
    Text(String),
    Number(f64),
}

'''
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-bitget',
           '--test', 'api_code_ingress', '--jobs', '1']
env = {**os.environ, 'RUSTC_BOOTSTRAP': '1', 'CARGO_BUILD_JOBS': '1'}
def run(label):
    result = subprocess.run(command, cwd=root, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    pathlib.Path(f'/tmp/bitget-api-code-{label}.log').write_text(result.stdout)
    return result.returncode, result.stdout

try:
    source.write_text(text[:start] + buffered + text[end:])
    status, output = run('buffered-control')
    assert status == 101 and 'error[E' not in output, output
    assert 'test api_code_direct_decimal_and_signed_zero_keep_native_number_bits ... FAILED' in output, output
    print('CONTROL: original buffered decoder compiles and fails the named direct-number witness', flush=True)
finally:
    source.write_bytes(original)
    assert source.read_bytes() == original
status, output = run('restored')
assert status == 0 and '2 passed; 0 failed' in output, output
print('RESTORED: both native API-code witnesses pass', flush=True)
