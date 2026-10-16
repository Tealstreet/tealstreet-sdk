#!/usr/bin/env python3
"""The shared factory's callbacks must reach the terminal and legacy resend."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-apexomni/src/binding/http.rs'
original = source.read_text()
name = 'shared_adapter_and_serializer_survive_initial_and_legacy_requests'
command = ['cargo', 'test', '-p', 'safe-cex-venue-apexomni', '--jobs', '1',
           '--test', 'binding', name, '--', '--exact']

def run():
    return subprocess.run(command, cwd=workspace, capture_output=True, text=True)

try:
    baseline = run()
    assert baseline.returncode == 0 and '1 passed' in baseline.stdout, baseline.stdout + baseline.stderr
    for before, after, label in [
        ('carry_config(&original, &before, &result)', 'Ok(js(&result))', 'initial adapter retention'),
        ('if value.is_callable() &&', 'if false && value.is_callable() &&', 'legacy adapter retention'),
    ]:
        assert original.count(before) == 1, before
        mutated = original.replace(before, after)
        if label.startswith('initial'):
            mutated = mutated.replace('fn carry_config(', '#[allow(dead_code)]\nfn carry_config(')
        source.write_text(mutated)
        result = run()
        output = result.stdout + result.stderr
        assert result.returncode == 101, output
        assert f'test {name} ... FAILED' in output and 'shared adapter through legacy replay' in output, output
        assert 'could not compile' not in output, output
        print('CONTROL PASS: ' + label + ' compiled and failed named E2E', flush=True)
        source.write_text(original)
        restored = run()
        assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout + restored.stderr
        print('RESTORED PASS: ' + label, flush=True)
finally:
    source.write_text(original)
