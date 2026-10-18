#!/usr/bin/env python3
"""Compile native receiver faults and restore the exact production bytes."""
from pathlib import Path
import subprocess

sdk = Path(__file__).resolve().parents[2]
source = sdk / "safe-cex-venue-bybit/src/binding"
controls = [
    ("getter", source / "host.rs",
     "fn get(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {\n        owner.get(key)",
     "fn get(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {\n        owner.get(key).map(|_| Js::Undefined)",
     "native_receiver_properties_literals_and_spreads"),
    ("owner", source / "hyro_socket.rs",
     "let owner = weak_object(&owner);\n    Ok(Js::function(code, move |_, args| {",
     "let retained_owner = owner.clone();\n    let owner = weak_object(&owner);\n    Ok(Js::function(code, move |_, args| {\n        let _keep_alive = &retained_owner;",
     "native_receiver_intrinsics_and_callback_effects"),
]
command = ["cargo", "test", "--locked", "-p", "safe-cex-venue-bybit", "--lib"]
for name, path, before, after, test in controls:
    original = path.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, (name, "source anchor changed")
    try:
        path.write_text(text.replace(before, after))
        result = subprocess.run(command + [test, "--jobs", "1"], cwd=sdk,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        Path(f"/tmp/bybit-receiver-{name}-red.log").write_text(result.stdout)
        assert result.returncode != 0 and "Finished `test`" in result.stdout, result.stdout
        assert f"{test} ... FAILED" in result.stdout, result.stdout
        print(f"{name}: compiled named native RED", flush=True)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
result = subprocess.run(command + ["receiver_tests", "--jobs", "1"], cwd=sdk,
                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
Path("/tmp/bybit-receiver-restored-green.log").write_text(result.stdout)
assert result.returncode == 0 and "3 passed; 0 failed" in result.stdout, result.stdout
print("exact restored production source: all 3 native receiver tests GREEN")
