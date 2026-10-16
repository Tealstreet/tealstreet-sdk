"""One compiled control per position/balance/order family; exact-byte restoration."""
import hashlib
import json
import os
import pathlib
import signal
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
CRATE = ROOT / "sdk-rs/safe-cex-venue-asterdex"
SOURCE = CRATE / "src/private_repairs.rs"
sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
subprocess.run(["git", "diff", "--exit-code", "HEAD", "--", str(CRATE / "src"), str(CRATE / "tests")], cwd=ROOT, check=True)
original = SOURCE.read_bytes()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH="/Users/sam/cs/tealstreet-next/node_modules/typescript")
controls = [
    ("position", "position_repairs_match_source", "let positions = fetching.await?;", "let mut positions = fetching.await?; if let Some(position) = positions.first_mut() { position.contracts = 1234.0; }", "position-stale-follow-up native position contracts"),
    ("balance", "balance_reconcile_matches_source", "repairs.schedule_balance_reconcile(repairs.parent.balance_state_version())?;", "repairs.schedule_balance_reconcile(version)?;", "balance-first-version-and-latest-retry native balance version"),
    ("order", "order_reconcile_matches_source", "self.owner.start_timeout(10_000,", "self.owner.start_timeout(1_000,", "order-debounce request count"),
]
record = {"input_sha": sha, "iteration": 1, "attempted_controls": 3, "original_sha256": hashlib.sha256(original).hexdigest(), "controls": []}
def interrupted(_signal, _frame):
    raise KeyboardInterrupt("control interrupted; restoring production source")
signal.signal(signal.SIGTERM, interrupted)
def run(argv, phase):
    print(phase, "anchored to", sha, flush=True)
    with subprocess.Popen(argv, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True) as process:
        try:
            output, _ = process.communicate()
        except BaseException:
            os.killpg(process.pid, signal.SIGTERM)
            process.communicate()
            raise
    print(output, flush=True)
    return {"exit": process.returncode, "output": output}
for family, test, needle, replacement, assertion in controls:
    text = original.decode()
    assert text.count(needle) == 1, (family, "mutation site changed")
    mutated = text.replace(needle, replacement, 1).encode()
    argv = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-asterdex", "--test", "live_private_repairs", "--jobs", "2", "--", "--exact", test, "--include-ignored", "--nocapture"]
    item = {"family": family, "test": test, "assertion": assertion, "argv": argv, "needle": needle, "replacement": replacement, "mutated_sha256": hashlib.sha256(mutated).hexdigest()}
    try:
        SOURCE.write_bytes(mutated)
        item["broken"] = run(argv, family + " broken")
    finally:
        SOURCE.write_bytes(original)
        assert SOURCE.read_bytes() == original
    item["restored_sha256"] = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    item["restored"] = run(argv, family + " restored")
    item["caught"] = item["broken"]["exit"] == 101 and "test result: FAILED" in item["broken"]["output"] and assertion in item["broken"]["output"]
    record["controls"].append(item)
    record["survivors"] = [c["family"] for c in record["controls"] if not c["caught"]]
    (CRATE / "PRIVATE_REPAIRS_CONTROLS.json").write_text(json.dumps(record, indent=2) + "\n")
    assert item["restored"]["exit"] == 0 and "test result: ok" in item["restored"]["output"]
    assert item["caught"], (family, "SURVIVOR or compiler failure; see recorded finding")
print("Iteration 1:3 controls; each compiled/assertion101/exact-restore0; survivors0", flush=True)
