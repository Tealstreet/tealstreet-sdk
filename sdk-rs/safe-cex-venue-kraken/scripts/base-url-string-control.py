#!/usr/bin/env python3
"""Remove only the string frontdoor, require the named native failure, restore."""
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-kraken/src/types.rs'
original = source.read_bytes()
guard = b'''        let tag = String::deserialize(deserializer)?;
        KrakenBaseUrlSerde::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(tag))'''
unguarded = b'        KrakenBaseUrlSerde::deserialize(deserializer)'
test = 'tagged_base_url_shapes_are_rejected_on_every_ingress_path'
assert original.count(guard) == 1, 'reviewed string-frontdoor body must be present exactly once'


def run(control=False):
    cmd = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-kraken', '--jobs', '1', '--test', 'accepted_base_url']
    if control:
        cmd += [test, '--', '--exact', '--nocapture']
    else:
        cmd += ['--test', 'types', '--', '--include-ignored']
    result = subprocess.run(cmd, cwd=workspace, env={**os.environ, 'RUSTC_BOOTSTRAP': '1'}, capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


baseline, output = run()
assert baseline.returncode == 0 and '2 passed; 0 failed; 0 ignored' in output and '4 passed; 0 failed; 0 ignored' in output, output[-8000:]
print('BASELINE PASS: native2 + actual-source types4, zero ignored', flush=True)
try:
    source.write_bytes(original.replace(guard, unguarded))
    changed, output = run(control=True)
    assert (changed.returncode != 0 and f'test {test} ... FAILED' in output
            and 'test result: FAILED' in output and 'could not compile' not in output
            and 'owned route admitted non-string {"derivatives":null}' in output), output[-8000:]
    print('CONTROL PASS: string-only frontdoor removed; compiled native tagged-shape witness fails', flush=True)
finally:
    source.write_bytes(original)
restored, output = run()
assert restored.returncode == 0 and '2 passed; 0 failed; 0 ignored' in output and '4 passed; 0 failed; 0 ignored' in output, output[-8000:]
assert source.read_bytes() == original
print('RESTORED PASS: exact source restored; native2 + actual-source types4, zero ignored', flush=True)
