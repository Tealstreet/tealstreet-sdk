#!/usr/bin/env python3
"""A compiling guard fault must break the native sign/bits/path witness."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-num/src/numeric_ingress.rs'
original = source.read_bytes()
condition = b'if number.to_bits() != actual.to_bits() {'
assert original.count(condition) == 1, 'exact reviewed output-number guard required'
command = ['cargo', 'test', '-p', 'safe-cex-num', '--test', 'numeric_output_invariant']


def run():
    result = subprocess.run(command, cwd=workspace, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, end='', flush=True)
    return result


try:
    source.write_bytes(original.replace(condition, b'if number.to_bits() == actual.to_bits() {'))
    result = run()
    assert result.returncode == 101, 'inverted bit guard must fail at runtime'
    assert 'test numeric_output_invariant_refuses_changed_bits_at_scalar_and_nested_paths ... FAILED' in result.stdout
    assert 'called `Result::unwrap_err()` on an `Ok` value' in result.stdout
finally:
    source.write_bytes(original)

assert source.read_bytes() == original, 'production source must be exactly restored'
assert run().returncode == 0, 'restored native invariant witness must pass'
print('Inverted Number-bit guard compiles and fails the named assertion; exact restoration passes.')
