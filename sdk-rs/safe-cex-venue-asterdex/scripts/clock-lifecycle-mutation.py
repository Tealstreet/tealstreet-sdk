"""One compiled generation-guard control for the new clock lifecycle family."""
import hashlib
import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
CRATE = ROOT / "sdk-rs/safe-cex-venue-asterdex"
SOURCE = CRATE / "src/virtual_clock.rs"
sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
subprocess.run(["git", "diff", "--exit-code", "HEAD", "--", str(CRATE / "src"), str(CRATE / "tests")], cwd=ROOT, check=True)
original = SOURCE.read_bytes()
text = original.decode()
start = text.index("pub fn sync_client_server_time_inner(")
needle = "if generation != clock.lifecycle_version.get() {"
assert text[start:].count(needle) == 1
mutated = (text[:start] + text[start:].replace(needle, "if false && generation != clock.lifecycle_version.get() {", 1)).encode()
argv = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-asterdex", "--test", "live_clock_lifecycle", "--jobs", "2", "--", "--exact", "clock_lifecycle_matches_source", "--include-ignored", "--nocapture"]
env = dict(os.environ)
env["SAFE_CEX_TYPESCRIPT_PATH"] = "/Users/sam/cs/tealstreet-next/node_modules/typescript"
record = {"input_sha": sha, "family": "virtual clock lifecycle", "iteration": 1, "attempted_controls": 1, "mutation": "disable the post-fetch generation guard in sync_client_server_time_inner", "argv": argv, "original_sha256": hashlib.sha256(original).hexdigest(), "mutated_sha256": hashlib.sha256(mutated).hexdigest()}
def run(phase):
    print(f"{phase} anchored to {sha}", flush=True)
    result = subprocess.run(argv, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    record[phase] = {"exit": result.returncode, "output": result.stdout}
    print(result.stdout, flush=True)
    return result
try:
    SOURCE.write_bytes(mutated)
    result = run("broken")
finally:
    SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    record["restored_sha256"] = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    restored = run("restored")
    (CRATE / "CLOCK_LIFECYCLE_MUTATION.json").write_text(json.dumps(record, indent=2) + "\n")
assert result.returncode == 101, "mutation must fail at a test assertion, not survive"
assert "test result: FAILED" in result.stdout
assert "stop-restart-coalesces-stale-request offset/generation: native 500.0, source 0.0" in result.stdout
assert restored.returncode == 0 and "test result: ok" in restored.stdout
print("Control caught: broken 101 -> exact-byte restore -> restored 0; survivors 0; stopped at iteration 1", flush=True)
