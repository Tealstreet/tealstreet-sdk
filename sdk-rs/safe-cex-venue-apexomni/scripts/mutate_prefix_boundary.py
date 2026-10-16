"""Prove prefix output and observed-surrogate assertions with restored controls."""
from pathlib import Path
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
out = root/'sdk-rs/target/apexomni-prefix-controls'/sha
out.mkdir(parents=True, exist_ok=True)
receipt = {'sha': sha, 'attempts': [], 'survivors': [], 'invalid_controls': []}


def run(name, expected, marker=None, extra=None):
    command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(root/'sdk-rs/Cargo.toml'),
               '--locked', '--jobs', '2', '-p', 'safe-cex-venue-apexomni', '--test', 'utils',
               'prefix_boundary_tests::prefix_boundary_observes_returned_units_before_wire',
               '--', '--exact', '--nocapture']
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, env={**os.environ, **(extra or {})}, capture_output=True, text=True)
    text = result.stdout + result.stderr
    (out/(name+'.log')).write_text(text)
    valid = result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name': name, 'exit': result.returncode, 'expected_exit': expected,
                               'assertion': marker, 'valid': valid, 'seconds': round(time.monotonic()-start,2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control':name,'call_site':'utils::drop_prefix'})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out/'controls.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print(name, result.returncode, 'valid', valid, flush=True)
    return valid


path = root/'sdk-rs/safe-cex-venue-apexomni/src/utils.rs'
original = path.read_bytes()
source = original.decode()
before = 'hash.strip_prefix("0x").unwrap_or(hash)'
assert source.count(before) == 1
try:
    path.write_text(source.replace(before, 'hash.strip_prefix("0X").unwrap_or(hash)'))
    run('native-prefix-mutation', 101, 'APEX-PREFIX pair native returned units')
finally:
    path.write_bytes(original)
assert run('native-prefix-restored', 0)
run('source-unit-substitution', 101, 'APEX-PREFIX high source returned units', {'SAFE_CEX_APEX_PREFIX_CONTROL':'substitute'})
assert run('source-unit-restored', 0)
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
assert not receipt['survivors'] and not receipt['invalid_controls'], receipt
