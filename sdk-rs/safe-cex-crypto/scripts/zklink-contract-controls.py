#!/usr/bin/env python3
"""Compiled negative controls; restore exact source even when a gate fails."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-crypto/src/zklink/contract.rs'
original = source.read_bytes()
text = original.decode()
controls = [
    ('fee-field-swap', 'bytes[33..35].copy_from_slice(&self.fee_rates);',
     'bytes[33..35].copy_from_slice(&[self.fee_rates[1], self.fee_rates[0]]);',
     'contract_fields_and_wire_signatures_match_npm', 'contract field/signature'),
    ('rounding-previous-exponent',
     'if number - mantissa * scale >= number - previous {',
     'if false && number - mantissa * scale >= number - previous {',
     'contract_amount_and_fee_rounding_match_npm', 'contract amounts rounding'),
    ('seed-recovery-byte', 'Ok(Self { seed })',
     'seed[64] = 27; Ok(Self { seed })',
     'contract_seed_bytes_match_npm_without_recovery_normalization', 'contract seed/signature'),
]

def run(name):
    return subprocess.run(
        ['cargo', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml',
         '-p', 'safe-cex-crypto', '--jobs', '1', '--test', 'zklink_contract',
         name, '--', '--exact'], cwd=root, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

try:
    for label, before, after, name, assertion in controls:
        assert text.count(before) == 1, label
        baseline = run(name)
        assert baseline.returncode == 0 and '1 passed' in baseline.stdout, baseline.stdout
        source.write_text(text.replace(before, after))
        negative = run(name)
        assert negative.returncode == 101, negative.stdout
        assert f'{name} ... FAILED' in negative.stdout, negative.stdout
        assert assertion in negative.stdout, negative.stdout
        assert 'could not compile' not in negative.stdout, negative.stdout
        print(f'{label}: compiled; named test/assertion failed (101)', flush=True)
        source.write_bytes(original)
        restored = run(name)
        assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout
        print(f'{label}: restored named test passed (0)', flush=True)
finally:
    source.write_bytes(original)
