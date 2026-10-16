#!/usr/bin/env python3
"""Sequential compiling mutations; exact restoration even on a failed control."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bybit/src/variant_accessors.rs'
evidence = root / 'sdk-rs/safe-cex-venue-bybit/evidence/variant-accessor-controls.json'
original = source.read_bytes()
gate_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--test', 'variant_accessors', '--jobs', '2', '--', '--include-ignored']
controls = [
    ('strict account type', '== Some(1.)', '== Some(6.)', 'VARIANT native account type'),
    ('constant category getter', 'vec!["inverse"]', 'vec!["linear"]', 'VARIANT native inverse categories'),
    ('missing-market warning', 'if !markets.is_empty() {', 'if markets.is_empty() {', 'VARIANT native category warning'),
    ('inverse ticker filter', 'market.quote != "USDT"', 'market.quote == "USDT"', 'VARIANT native ticker result'),
    ('undefined settlement element', 'return Ok(vec![None]);', 'return Ok(vec![Some("USDT")]);', 'VARIANT native undefined settle coin'),
    ('finite numeric predicate', '.is_finite()', '.is_nan()', 'VARIANT native finite numeric'),
    ('settlement logging', 'emit_and_log_error(&message)?;', 'let _ = emit_and_log_error;', 'VARIANT native settle logging'),
]
results = []
def run():
    return subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
try:
    baseline = run()
    assert baseline.returncode == 0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout, baseline.stdout
    for name, before, after, assertion in controls:
        text = original.decode()
        expected_occurrences = 2 if name == 'settlement logging' else 1
        assert text.count(before) == expected_occurrences, (name, text.count(before))
        source.write_text(text.replace(before, after, 1))
        broken = run()
        detected = broken.returncode == 101 and 'panicked at' in broken.stdout and assertion in broken.stdout
        source.write_bytes(original)
        restored = run()
        passed = restored.returncode == 0 and '1 passed; 0 failed; 0 ignored' in restored.stdout
        result = dict(family=name, assertion=assertion, mutation_exit=broken.returncode, named_runtime_failure=detected, restored_exit=restored.returncode, restored_pass=passed)
        results.append(result)
        print(json.dumps(result), flush=True)
        if not detected or not passed:
            print(broken.stdout, restored.stdout, flush=True)
            raise AssertionError('surviving or invalid control: ' + name)
finally:
    source.write_bytes(original)
    evidence.parent.mkdir(parents=True, exist_ok=True)
    evidence.write_text(json.dumps(dict(gate_sha=gate_sha, baseline_exit=baseline.returncode, command=command, controls=results, enumeration_floor=7), indent=2) + '\n')
assert source.read_bytes() == original
print('Seven controls detected; exact production restoration verified.', flush=True)
