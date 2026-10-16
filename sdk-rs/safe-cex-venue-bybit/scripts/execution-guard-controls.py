#!/usr/bin/env python3
"""Seven new-function controls, one Cargo invocation at a time."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bybit/src/execution_guards.rs'
evidence = root / 'sdk-rs/safe-cex-venue-bybit/evidence/execution-guard-controls.json'
original = source.read_bytes()
gate_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--test', 'execution_guards', '--jobs', '2', '--', '--include-ignored']
controls = [
    ('request rejection flag', 'needs_reinit.set(true);', 'needs_reinit.set(false);', 'GUARD native rejection flag'),
    ('window stepping', 'window_end = window_start - 1.;', 'window_end = window_start + 1.;', 'GUARD native window count'),
    ('caught-error logging', 'emit_and_log_error(&message)?;', 'emit_and_log_error(&format!("{message} changed"))?;', 'GUARD native execution callback order'),
    ('spot execution category', 'get_bybit_order_execution_params(opts, "spot")', 'get_bybit_order_execution_params(opts, "linear")', 'GUARD native execution rejection'),
    ('position mode selection', '== Some("oneway")', '== Some("hedged")', 'GUARD native number position index'),
    ('spot constant index', 'pub async fn spot_get_order_position_idx<T>(_opts: &T) -> f64 {\n    0.\n}', 'pub async fn spot_get_order_position_idx<T>(_opts: &T) -> f64 {\n    1.\n}', 'GUARD native number position index'),
    ('stop omit forwarding', 'without_reduce_only.remove("reduceOnly");', 'without_reduce_only.remove("other");', 'GUARD native omitted reduceOnly'),
]
results = []
def run():
    return subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
baseline = run()
assert baseline.returncode == 0 and '1 passed; 0 failed; 0 ignored' in baseline.stdout, baseline.stdout
try:
    for name, before, after, assertion in controls:
        text = original.decode()
        assert text.count(before) == 1, (name, text.count(before))
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
