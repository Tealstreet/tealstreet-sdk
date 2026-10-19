#!/usr/bin/env python3
"""Compile two position-index faults through the real exchange receivers."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
base = root / "safe-cex-venue-bybit/src/binding/members"
files = {name: (base / name).read_bytes() for name in ["batch.rs", "spot.rs"]}
faults = [
    ("batch.rs", b'method(rt, "getOrderPositionIdx", |rt, a| {\n        let opts = arg(&a, 0);', b'method(rt, "getOrderPositionIdx", |rt, a| {\n        let opts = arg(&a, 0).spread_object()?;',
     "post-await reads see the same options graph"),
    ("spot.rs", b'Ok(rt.spawn(Box::pin(async { Ok(Js::Number(0.)) })))',
     b'Ok(rt.spawn(Box::pin(async { Ok(Js::Number(1.)) })))',
     "strict_equals(&Js::Number(0.))"),
]
command = ["cargo", "test", "--locked", "-p", "safe-cex-venue-bybit", "--lib",
           "divergence_audit_order_index_keeps_live_options_and_rejection_identity",
           "--jobs", "2", "--", "--test-threads=1"]
logs = Path(tempfile.mkdtemp(prefix="bybit-position-controls-"))
env = dict(os.environ, CARGO_INCREMENTAL="0")
for i, (name, before, after, assertion) in enumerate(faults):
    original = files[name]
    assert original.count(before) == 1
    try:
        (base / name).write_bytes(original.replace(before, after))
        with (logs / f"mutant-{i}.log").open("w") as output:
            result = subprocess.run(command, cwd=root, env=env, stdout=output,
                                    stderr=subprocess.STDOUT, timeout=300)
        text = (logs / f"mutant-{i}.log").read_text()
        assert result.returncode == 101 and "test result: FAILED" in text, text
        assert assertion in text, text
    finally:
        (base / name).write_bytes(original)
    with (logs / f"restored-{i}.log").open("w") as output:
        result = subprocess.run(command, cwd=root, env=env, stdout=output,
                                stderr=subprocess.STDOUT, timeout=300)
    assert result.returncode == 0, (logs / f"restored-{i}.log").read_text()
    assert (base / name).read_bytes() == original
print("PASS: both compiled receiver faults fail; exact restores pass")
print(logs)
