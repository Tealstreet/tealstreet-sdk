#!/usr/bin/env python3
"""Prove original emitter aliases and eager calls in both constructed families."""
import os
from pathlib import Path
import subprocess
import tempfile

package = Path(__file__).resolve().parents[1]
workspace = package.parent
logs = Path(tempfile.mkdtemp(prefix="hyperliquid-shared-host-"))
env = {**os.environ, "CARGO_INCREMENTAL": "0"}
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-hyperliquid", "--lib"]
identity = "host::tests::production_emitter_preserves_empty_array_and_multiple_arguments"
eager = "host::socket_tests::both_constructed_families_keep_original_js_arguments_and_start_async_calls_eagerly"
launch = """safe_cex_exchange::eager::start_eager(
            &self.base.runtime,
            async move { body(binding?).await },
        )"""
emit = "b.emit_args(&event, args.get(1..).unwrap_or_default())"
copy = """b.emit_args(&event, &args.get(1..).unwrap_or_default().iter().map(|value| {
                if value.is_array() { Ok(Js::array(value.array_values()?.expect("array"))) }
                else { Ok(value.clone()) }
            }).collect::<Result<Vec<_>, Thrown>>()?)"""
cases = [
    ("emitter-array-copy", "src/host/facade_exchange.rs", emit, copy, identity, "HL_REALM_ORIGINAL_ARRAY_IDENTITY"),
    ("perp-lazy-launch", "src/binding.rs", launch, "Box::pin(async move { body(binding?).await })", eager, "public async prefix must run before the waiter is polled: spot=false"),
    ("spot-lazy-launch", "src/spot_binding.rs", launch, "Box::pin(async move { body(binding?).await })", eager, "public async prefix must run before the waiter is polled: spot=true"),
]

def run(label, target):
    result = subprocess.run(command + [target, "--", "--exact"], cwd=workspace, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (label + ".log")).write_text(result.stdout)
    return result

for label, relative, before, after, target, assertion in cases:
    source = package / relative
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, (label, text.count(before))
    try:
        source.write_text(text.replace(before, after))
        broken = run(label, target)
        assert broken.returncode == 101 and "error[E" not in broken.stdout, broken.stdout
        assert "test result: FAILED. 0 passed; 1 failed" in broken.stdout and assertion in broken.stdout, broken.stdout
        print(label + ": compiled named RED", flush=True)
    finally:
        source.write_bytes(original)
    restored = run(label + "-restored", target)
    assert restored.returncode == 0 and "1 passed; 0 failed" in restored.stdout, restored.stdout
    assert source.read_bytes() == original
    print(label + ": exact restoration GREEN", flush=True)
print(logs)
