"""Consume every retained array element before yielding None; always restore.

Second control keeps the production mutation and bypasses BOTH source and
native array-length checks. Full decoded-native equality remains mandatory.
This is a mutation of healthy production, not a claim of a live decoder bug.
"""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bitrue'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
production = root / 'sdk-rs/safe-cex-num/src/numeric_ingress.rs'
helper = venue / 'tests/cardinality.rs.inc'
originals = {path: path.read_text() for path in (production, helper)}
needle = '''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
replacement = '''        let _ = seed;
        for _ in self.0.by_ref() {}
        Ok(None)'''
assert originals[production].count(needle) == 1
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path',
           str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitrue',
           '--locked', '--jobs', '1', '--test', 'wire']
for bypass in (False, True):
    try:
        production.write_text(originals[production].replace(needle, replacement))
        if bypass:
            text = originals[helper]
            for line in text.splitlines():
                if 'assert_eq!(array_cardinalities(' in line:
                    text = text.replace(line, '    let _ = ' + (
                        '(array_cardinalities(value), expected, target);'
                        if 'array_cardinalities(value)' in line else
                        '(array_cardinalities(actual), array_cardinalities(expected), target);'))
            helper.write_text(text)
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assertion = ('native type parity:' if bypass else
                     'BITRUE-NATIVE-CARDINALITY expected/native: wire')
        assert result.returncode == 101 and assertion in output and 'could not compile' not in output, output
        print(f'CONSUMING EMPTY guards_bypassed={bypass}: compiled EXIT101 {assertion}', flush=True)
    finally:
        for path, text in originals.items():
            path.write_text(text)
    result = subprocess.run(command, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    print('RESTORED PASS wire', flush=True)
