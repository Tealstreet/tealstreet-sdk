"""Sequential compiled red/restored controls; failures must name a native assertion."""
import hashlib
import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / "sdk-rs/safe-cex-venue-woo/src/public_parity.rs"
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH="/Users/sam/cs/tealstreet-next/node_modules/typescript")
CARGO = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml",
         "-p", "safe-cex-venue-woo", "--test", "public_parity_core", "--jobs", "2"]
original = SOURCE.read_bytes()
text = original.decode()
sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
controls = [
    ("route-decode-symmetric-swap", "decoded_native_routes_and_field_same_value_semantics", "native decoded route variant",
     text.replace('#[serde(rename = "markets")]', '#[serde(rename = "SWAP")]', 1)
         .replace('#[serde(rename = "tickers")]', '#[serde(rename = "markets")]', 1)
         .replace('#[serde(rename = "SWAP")]', '#[serde(rename = "tickers")]', 1)),
    ("same-value-zero-sign", "decoded_native_routes_and_field_same_value_semantics", "native Object.is mismatch count case 1",
     text.replace('(a == b && (*a != 0.0 || a.is_sign_negative() == b.is_sign_negative()))', '(a == b)', 1)),
    ("key-insertion-order", "keyed_native_first_winner_order_and_stored_undefined", "native keyed insertion order OTHER before 42",
     text.replace('    for (key, legacy_value) in &legacy_by_key {',
                  '    legacy_by_key.sort_by(|a, b| a.0.cmp(&b.0));\n    for (key, legacy_value) in &legacy_by_key {', 1)),
    ("blocker-priority", "native_blocker_presence_result_priority_and_snapshot_identity", "native result blocker priority case 2",
     text.replace('let status = if !blockers.is_empty() {\n        ParityStatus::Blocked',
                  'let status = if !blockers.is_empty() {\n        ParityStatus::Mismatch', 1)),
]
records = []
def run(name, phase, test):
    cmd = CARGO + [test, "--", "--exact"]
    p = subprocess.run(cmd, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    log = ROOT / f".p6-review/public-core-{name}-{phase}.log"
    log.write_text(p.stdout)
    # The assertion and test footer are retained in the committed results, not just ignored logs.
    lines = p.stdout.splitlines()
    excerpt = "\n".join(lines[-28:])
    return {"command": cmd, "exit": p.returncode, "output_excerpt": excerpt}
try:
    for name, test, assertion, mutant in controls:
        assert mutant != text, ("mutation applied", name)
        SOURCE.write_text(mutant)
        red = run(name, "red", test)
        SOURCE.write_bytes(original)
        restored = run(name, "restored", test)
        caught = (red["exit"] == 101 and assertion in red["output_excerpt"]
                  and "test result: FAILED" in red["output_excerpt"] and restored["exit"] == 0)
        records.append({"name": name, "gate_sha": sha, "overlay": "deliberate uncommitted production mutation",
                        "test": test, "named_assertion": assertion, "red": red, "restored": restored,
                        "caught": caught, "source_restored_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest()})
        print(json.dumps({"name": name, "red": red["exit"], "restored": restored["exit"], "caught": caught}), flush=True)
        if not caught:
            raise AssertionError(("SURVIVOR OR INVALID CONTROL", name))
finally:
    SOURCE.write_bytes(original)
    (ROOT / ".p6-review/public-core-controls.json").write_text(json.dumps({
        "sha": sha, "source_original_sha256": hashlib.sha256(original).hexdigest(),
        "controls": records, "survivors": [r["name"] for r in records if not r["caught"]],
        "floor": "four listed compiled controls; not exhaustive mutation enumeration",
    }, indent=2) + "\n")
