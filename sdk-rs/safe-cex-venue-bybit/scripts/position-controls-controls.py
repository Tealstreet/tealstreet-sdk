#!/usr/bin/env python3
"""Five new-function controls, one Cargo invocation at a time."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bybit/src/position_controls.rs'
evidence = root / 'sdk-rs/safe-cex-venue-bybit/evidence/position-controls-controls.json'
original = source.read_bytes()
gate_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--test', 'position_controls', '--jobs', '2', '--', '--include-ignored']
controls = [('leverage clamp', 'let leverage = math_min(math_max(input_leverage, min), max);', 'let leverage = math_min(math_max(input_leverage, min), max + 1.);', 'POSITION native leverage event'), ('mode bulk route', 'helper(hedged, "linear", "USDT")?.await', 'helper(hedged, "inverse", "USDT")?.await', 'POSITION native mode event'), ('mode helper hedged index', 'Value::from(if hedged { 3 } else { 0 })', 'Value::from(if hedged { 4 } else { 0 })', 'POSITION native helper event'), ('margin setter', 'set_margin_type(if isolated {\n        MarginType::Isolated', 'set_margin_type(if isolated {\n        MarginType::Cross', 'POSITION native margin event'), ('first fetched mode', 'Some(first) => first.get("positionMode").cloned(),', 'Some(_first) => Some(Value::String("oneway".into())),', 'POSITION native fetch result')]
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
    evidence.write_text(json.dumps(dict(gate_sha=gate_sha, baseline_exit=baseline.returncode, command=command, controls=results, enumeration_floor=5), indent=2) + '\n')
assert source.read_bytes() == original
print('Five controls detected; exact production restoration verified.', flush=True)
