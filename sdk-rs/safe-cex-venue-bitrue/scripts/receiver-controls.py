#!/usr/bin/env python3
"""Compile independent production receiver faults; always restore exact source bytes.

Run alone, from sdk-rs, with one Cargo process. A STOP file in the output directory
is honored between restored cases, so the serialized array cutover can take priority.
"""
import hashlib
import json
from pathlib import Path
import subprocess

SDK = Path(__file__).resolve().parents[2]
CRATE = SDK / "safe-cex-venue-bitrue"
OUT = SDK / "target/bitrue-receiver-controls"
OUT.mkdir(parents=True, exist_ok=True)
CASES = [
    ("collection", "host.rs", "ReadHost", '.get(&index.to_string())', '.get(&(index + 1).to_string())', "collection_receiver_preserves_handles_keys_and_native_bits"),
    ("enum-lookup", "sockets.rs", "ParseHost", 'Rt::global(self, t)?.get_key(&k)', 'Rt::global(self, t)?.get_key(&k.iter().copied().chain([33]).collect::<Vec<_>>())', "socket_receivers_keep_lookup_identity_and_callback_order"),
    ("subscription", "sockets.rs", "SubscriptionHost", 'for c in channels {', 'for c in channels.iter().rev() {', "socket_receivers_keep_lookup_identity_and_callback_order"),
    ("public-global", "sockets.rs", "PublicBodyHost", 'Rt::global(self, name)', 'Rt::global(self, if name == "ORDER_SIDE" { "ORDER_TYPE" } else { name })', "socket_receivers_keep_lookup_identity_and_callback_order"),
    ("stream-global", "sockets.rs", "StreamHost", 'Rt::global(self, n)', 'Rt::global(self, if n == "ORDER_SIDE" { "ORDER_TYPE" } else { n })', "socket_receivers_keep_lookup_identity_and_callback_order"),
    ("lifecycle", "host.rs", "LifecycleHost", 'self.global(name)', 'self.global(if name == "setTimeout" { "clearTimeout" } else { name })', "lifecycle_and_trading_receivers_keep_presence_and_delete_semantics"),
    ("trading-delete", "host.rs", "TradingHost", 'v.delete(&key.encode_utf16().collect::<Vec<_>>())?;', 'v.get_key(&key.encode_utf16().collect::<Vec<_>>())?;', "lifecycle_and_trading_receivers_keep_presence_and_delete_semantics"),
    ("private-hmac", "private.rs", "PrivateHost", 'global_call(self, "createHmac", vec![Js::str(algorithm), secret])', 'global_call(self, "createHmac", vec![Js::str(algorithm), ops::add(&secret, &Js::str("fault"))?])', "private_receiver_forwards_native_strings_hmac_and_members"),
    ("composition", "classes.rs", "CompositionHost", 'if which == "every" {', 'if which != "every" {', "composition_receiver_uses_live_factories_and_lexical_members"),
]


def run(name, test):
    log = OUT / f"{name}.log"
    with log.open("w") as output:
        result = subprocess.run(["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-bitrue", "--lib", test, "--", "--nocapture"], cwd=SDK, stdout=output, stderr=subprocess.STDOUT, check=False)
    return result.returncode, log.read_text()


results = []
status, _ = run("baseline", "receiver_tests")
assert status == 0, "baseline native witnesses must pass before faults"
for role, filename, trait, before, after, test in CASES:
    if (OUT / "STOP").exists():
        break
    path = CRATE / "src/binding" / filename
    original = path.read_bytes()
    source = original.decode()
    begin = source.index(f"impl {trait} for Rt {{")
    end = source.index("\n}", begin) + 2
    body = source[begin:end]
    assert body.count(before) == 1, f"{role}: exact reviewed mutation changed"
    try:
        path.write_text(source[:begin] + body.replace(before, after, 1) + source[end:])
        status, log = run(f"{role}-compiled-red", test)
        assert status != 0 and "could not compile" not in log and "test result: FAILED" in log and test in log, f"{role}: need compiled named assertion failure"
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    status, _ = run(f"{role}-restored-green", test)
    assert status == 0, f"{role}: exact restoration must pass"
    results.append({"role": role, "test": test, "compiledRed": True, "restoredGreen": True, "sourceSha256": hashlib.sha256(original).hexdigest()})
    (OUT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{role}: compiled RED / exact restored GREEN", flush=True)
assert len(results) == len(CASES), "interrupted with source restored; remaining controls are not claimed"
