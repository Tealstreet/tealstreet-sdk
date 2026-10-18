#!/usr/bin/env python3
"""Seven independent real-production alias controls; run with no other Cargo build."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

SDK = Path(__file__).resolve().parents[2]
CRATE = SDK / "safe-cex-venue-bitrue"
OUT = SDK / "target/bitrue-public-alias-controls"
OUT.mkdir(parents=True, exist_ok=True)
SYNC = "public_and_collection_callbacks_retain_original_native_values_and_errors"
ASYNC = "async_callback_and_body_keep_pending_resolution_and_rejection_identity"
HANDLE = "metadata_handle_alias_retains_real_cache_aliases_and_all_fields"
CASES = [
    ("native-callback", "binding/sockets.rs", "PublicBodyHost", 'Js::function("", move |_, a| f(a))', 'Js::function("", move |_, a| { f(a)?; Ok(Js::Undefined) })', SYNC),
    ("collection-callback", "binding/host.rs", "ReadHost", 'Js::function("", move |_, a| f(a))', 'Js::function("", move |_, a| { f(a)?; Ok(Js::Undefined) })', SYNC),
    ("async-callback", "binding/host.rs", "TradingHost", 'Promise::spawn(&runtime, f(a))', 'Promise::spawn(&runtime, f(a.into_iter().rev().collect()))', ASYNC),
    ("body-future", "binding/host.rs", "TradingHost", 'Promise::spawn(&runtime, f(a))', 'Promise::spawn(&runtime, Box::pin({ let future = f(a); async move { future.await?; Ok(Js::Undefined) } }))', ASYNC),
    ("metadata-handle", "position_metadata.rs", None, '.map(|(_, entry)| entry.clone())', '.map(|(_, entry)| Rc::new(RefCell::new(entry.borrow().clone())))', HANDLE),
]


def run(name, command, bootstrap=False):
    path = OUT / (name + ".log")
    env = os.environ.copy()
    if bootstrap:
        env["RUSTC_BOOTSTRAP"] = "1"
    with path.open("w") as output:
        result = subprocess.run(command, cwd=SDK, env=env, stdout=output, stderr=subprocess.STDOUT, check=False)
    return result.returncode, path.read_text()


def native(test):
    return ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-bitrue", "--lib", test, "--", "--nocapture"]


def aliases(flag=None):
    return ["node", "safe-cex-venue-bitrue/scripts/public-alias-contracts.mjs"] + ([flag] if flag else [])


status, _ = run("native-baseline", native("public_aliases"))
assert status == 0, "actual native consumers must pass before faults"
status, _ = run("compiler-baseline", aliases(), True)
assert status == 0, "canonical seven-owner baseline must pass"
results = []
for role, filename, trait, before, after, test in CASES:
    if (OUT / "STOP").exists():
        break
    path = CRATE / "src" / filename
    original = path.read_bytes()
    source = original.decode()
    begin = source.index(f"impl {trait} for Rt {{") if trait else 0
    end = source.index("\n}", begin) + 2 if trait else len(source)
    body = source[begin:end]
    assert body.count(before) == 1, role + ": exact mutation changed"
    try:
        path.write_text(source[:begin] + body.replace(before, after, 1) + source[end:])
        status, log = run(role + "-compiled-red", native(test))
        assert status != 0 and "could not compile" not in log and "test result: FAILED" in log and test in log, role + ": need compiled named native failure"
    finally:
        path.write_bytes(original)
    status, _ = run(role + "-restored-green", native(test))
    assert status == 0 and path.read_bytes() == original, role + ": exact restoration must pass"
    results.append({"role": role, "test": test, "compiledRed": True, "restoredGreen": True, "sourceSha256": hashlib.sha256(original).hexdigest()})
    (OUT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(role + ": compiled RED / exact restored GREEN", flush=True)

for role, record, first, second in [
    ("contracts-response", "FetchContractsResponseItemWire", "pricePrecision", "side"),
    ("kline-response", "FetchKlineResponseItemWire", "high", "low"),
]:
    if (OUT / "STOP").exists():
        break
    path = CRATE / "src/wire.rs"
    original = path.read_bytes()
    source = original.decode()
    begin = source.index("struct " + record + " {")
    end = source.index("\n}", begin) + 2
    body = source[begin:end]
    a = f'#[serde(rename = "{first}")]'
    b = f'#[serde(rename = "{second}")]'
    assert body.count(a) == body.count(b) == 1
    owner = "wire::FetchContractsResponse" if role == "contracts-response" else "wire::FetchKlineResponse"
    try:
        path.write_text(source[:begin] + body.replace(a, "__ALIAS_SWAP__").replace(b, a).replace("__ALIAS_SWAP__", b) + source[end:])
        status, log = run(role + "-compiled-red", aliases("--expect-mismatch=" + owner), True)
        assert status == 0 and "Compiled native mismatch detected" in log, role + ": need actual canonical native-field failure"
        receipt = json.loads((SDK / "target/bitrue-public-alias-results.json").read_text())
        assert [row["name"] for row in receipt if row["failures"]] == ["safe_cex_venue_bitrue::" + owner]
        assert not any("could not compile" in failure for row in receipt for failure in row["failures"])
        (OUT / (role + "-compiled-red-results.json")).write_text(json.dumps(receipt, indent=2) + "\n")
    finally:
        path.write_bytes(original)
    status, _ = run(role + "-restored-green", aliases(), True)
    assert status == 0 and path.read_bytes() == original, role + ": exact restoration must pass"
    results.append({"role": role, "owner": owner, "compiledRed": True, "restoredGreen": True, "sourceSha256": hashlib.sha256(original).hexdigest()})
    (OUT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(role + ": canonical compiled RED / exact restored GREEN", flush=True)
assert len(results) == 7, "interrupted with source restored; remaining controls are not claimed"
