#!/usr/bin/env python3
"""Fault canonical private construction and Auth forwarding independently."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / "src/private_classes.rs"
original = source.read_bytes()
logs = workspace / "target/private-owner-controls"
logs.mkdir(parents=True, exist_ok=True)
command = ["cargo", "test", "--color", "never", "--locked", "--jobs", "1",
           "-p", "safe-cex-venue-binance", "--features", "native", "--test", "private_classes"]
receipt = {"sourceSha256": hashlib.sha256(original).hexdigest(), "controls": []}


def execute(label, name):
    output = logs / (label + ".log")
    with output.open("w") as stream:
        status = subprocess.run(command + [name, "--", "--exact"], cwd=workspace,
                                stdout=stream, stderr=subprocess.STDOUT).returncode
    return status, output.read_text()


for role, owners, before, after, name in [
    ("constructor", ["PrivateSocket", "PrivateState"],
     "this: weak.clone(),\n                venue,",
     "this: weak.clone(),\n                venue: Venue::Futures,",
     "private_owner_binding::native_private_owner_keeps_family_receiver_defaults_and_store_route"),
    ("auth", ["Auth"],
     "crate::api_factory::utf8_hmac_sha256_hex(secret, payload)",
     'crate::api_factory::utf8_hmac_sha256_hex(secret, format!("{payload}-fault"))',
     "private_owner_binding::native_spot_auth_uses_actual_parent_clock_secret_and_socket"),
]:
    status, log = execute(role + "-baseline-green", name)
    assert status == 0 and "1 passed; 0 failed" in log, log[-3000:]
    text = original.decode()
    assert text.count(before) == 1, role
    try:
        source.write_text(text.replace(before, after))
        status, log = execute(role + "-red", name)
        assert status != 0 and f"test {name} ... FAILED" in log
        assert not re.search(r"^error(?:\[|: (?!test failed\b))", log, re.M), log[-3000:]
        assert "0 passed; 1 failed" in log
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(role + "-exact-restoration-green", name)
    assert status == 0 and "1 passed; 0 failed" in log, log[-3000:]
    receipt["controls"].append({"owners": ["safe_cex_venue_binance::private_classes::" + n for n in owners],
                                "test": name, "compiledNamedRed": True,
                                "exactRestorationGreen": True})
    (logs / "results.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(role + ": compiled named native RED; exact restoration GREEN", flush=True)
