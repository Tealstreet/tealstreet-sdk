import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-venue-coinbase/src/binding/lifecycle.rs"
original = source.read_bytes()
before = b"Ok(Rc::new(RefCell::new(timer)))"
after = b"Ok(Rc::new(RefCell::new(safe_cex_exchange::js_runtime::value::from_json(&safe_cex_exchange::js_runtime::value::to_json(&timer).map_err(from_thrown)?))))"
assert original.count(before) == 1
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-coinbase",
           "--lib", "lifecycle_timer_native"]

def run():
    return subprocess.run(command, cwd=workspace, env={**os.environ, "CARGO_BUILD_JOBS": "1"},
                          text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

try:
    source.write_bytes(original.replace(before, after))
    failed = run()
    Path("/tmp/coinbase-lifecycle-timer-reversion-red.log").write_text(failed.stdout)
    assert failed.returncode != 0 and "panicked at" in failed.stdout, failed.stdout
    for name in ["coinbase", "coinbase_spot"]:
        assert name + "_timer_cancellation_retains_the_real_runtime_handle ... FAILED" in failed.stdout, failed.stdout
finally:
    source.write_bytes(original)

assert source.read_bytes() == original
restored = run()
Path("/tmp/coinbase-lifecycle-timer-restored.log").write_text(restored.stdout)
assert restored.returncode == 0 and "2 passed; 0 failed" in restored.stdout, restored.stdout
print("Both Coinbase variants: compiled JSON-roundtrip reversion RED; exact source restoration GREEN")
