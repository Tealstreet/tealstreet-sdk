#!/usr/bin/env python3
"""Five new-function controls, one Cargo invocation at a time."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bybit/src/public_ticker_topics.rs'
evidence = root / 'sdk-rs/safe-cex-venue-bybit/evidence/public-ticker-topics-controls.json'
original = source.read_bytes()
gate_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--test', 'public_ticker_topics', '--jobs', '2', '--', '--include-ignored']
controls = [('current array identity', 'Some(current) if current.borrow().is_array() => Ok(current),', 'Some(current) if current.borrow().is_array() => Ok(Rc::new(RefCell::new(current.borrow().clone()))),', 'TOPICS native current array identity'), ('desired topic prefix', '.map(|symbol| format!("tickers.{symbol}"))\n        .collect())', '.map(|symbol| format!("ticker.{symbol}"))\n        .collect())', 'TOPICS native desired topic'), ('ordered deduplication', '.filter(|topic| seen.insert(topic.clone()))', '.filter(|topic| { seen.insert(topic.clone()); true })', 'TOPICS native topic count'), ('active chunk order', 'for topics in subscribe.chunks(10) {', 'for topics in subscribe.chunks(10).rev() {', 'TOPICS native topic count'), ('market chunk sleep', 'sleep(5).await?;', 'sleep(6).await?;', 'TOPICS native sleep delay')]
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
