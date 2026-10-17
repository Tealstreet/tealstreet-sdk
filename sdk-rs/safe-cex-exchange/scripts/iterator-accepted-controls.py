#!/usr/bin/env python3
"""Each reviewed owner must lose its native witness under a compiled fault."""
from pathlib import Path
import os
import json
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-exchange/src/js_runtime"
logs = Path(tempfile.mkdtemp(prefix="iterator-accepted-controls-"))
print(f"Control logs: {logs}", flush=True)
# The fresh selected gate must pass before this runner. Capture its compiler
# document once; the finite faults below change one body line, never an API.
# The ordinary probe runner always inventories fresh production source.
document = root / "sdk-rs/target/doc/safe_cex_exchange.json"
assert document.is_file(), "run iterator-accepted-probes.mjs before these controls"
snapshot = logs / "compiler.json"
snapshot.write_bytes(document.read_bytes())
probe = logs / "probe.mjs"
probe.write_text(f'''import assert from 'node:assert/strict';
import fs from 'node:fs';
import {{spawnSync}} from 'node:child_process';
import {{inventory}} from {json.dumps((root / 'sdk-rs/safe-cex-parity/scripts/accepted-inventory.mjs').as_uri())};
import {{runProbes}} from {json.dumps((root / 'sdk-rs/safe-cex-parity/scripts/accepted-probes.mjs').as_uri())};
import {{loadContracts}} from {json.dumps((root / 'sdk-rs/safe-cex-parity/scripts/accepted-contract-files.mjs').as_uri())};
const workspace={json.dumps(str(root / 'sdk-rs'))};
const owner=process.argv[2];
const owners=['safe_cex_exchange::js_runtime::builtins::cached_iterator_member','safe_cex_exchange::js_runtime::collections::Collection','safe_cex_exchange::js_runtime::collections::iterator_next_method','safe_cex_exchange::js_runtime::collections::iterator_object','safe_cex_exchange::js_runtime::value::Js'];
const selected=owner==='all'?owners:[owner];
assert.ok(selected.every(owner=>owners.includes(owner)));
const command=spawnSync('cargo',['metadata','--locked','--format-version=1'],{{cwd:workspace,encoding:'utf8',timeout:900000,maxBuffer:64*1024*1024}});
assert.equal(command.status,0,command.stderr);
const metadata=JSON.parse(command.stdout);
const doc=JSON.parse(fs.readFileSync({json.dumps(str(snapshot))}));
const found=inventory(doc,{{onlyOwners:selected}});
const receipts=runProbes(workspace,metadata,[{{...found,package:'safe-cex-exchange'}}],loadContracts(),{{target:'native',features:[],jobs:1}});
assert.equal(receipts.length,selected.length);
for(const receipt of receipts) console.log(JSON.stringify(receipt));
assert.equal(receipts.filter(receipt=>receipt.failures.length).length,0,'body-only control: accepted output observation must pass');
''')
command = ["cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "--locked", "--jobs", "1",
           "-p", "safe-cex-exchange", "--lib"]
faults = [
    ("cached-member", "builtins.rs", b"return Js::Function(function);",
     b"return { let _ = function; member };", "accepted_iterator_cached_member_retains_live_method",
     "cached_iterator_member native identity and invocation"),
    ("collection", "collections.rs", b"self.values.push(value);", b"let _ = value;",
     "accepted_iterator_collection_dense_view", "Collection native ordered view"),
    ("iterator-object", "collections.rs", b"*data.iterator_next.borrow_mut() = Some(next);",
     b"*data.iterator_next.borrow_mut() = { let _ = next; None };",
     "accepted_iterator_object_retains_protocol_and_native_step", "iterator_object native protocol fields"),
    ("next-receiver", "collections.rs", b"if data.class == brand {", b"if data.class != brand {",
     "accepted_iterator_next_uses_actual_branded_receiver", "iterator_next_method actual receiver and refusal"),
    ("computed-symbol", "value.rs", b"return super::collections::iterator_method(self);",
     b"return { let _ = super::collections::iterator_method(self)?; Ok(Js::Undefined) };", "accepted_iterator_js_protocol_entrypoints", "Js::get_property native symbol front door"),
    ("get-iterator", "value.rs", b"        Ok(iterator)\n", b"        Ok({ let _ = iterator; Js::Number(0.0) })\n",
     "accepted_iterator_js_protocol_entrypoints", "Js::iterator native protocol result"),
    ("iterable-values", "value.rs", b'values.push(item.get("value")?);', b"values.push(Js::Null);",
     "accepted_iterator_js_protocol_entrypoints", "Js::iterable_values native Map/Set protocol"),
]
original = {source / filename: (source / filename).read_bytes() for _, filename, *_ in faults}


def run(label, test, count):
    log = logs / f"{label}.txt"
    with log.open("w") as output:
        result = subprocess.run(command + [test, "--", "--test-threads=1"], cwd=root, env=os.environ,
                                stdout=output, stderr=subprocess.STDOUT, timeout=900)
    text = log.read_text()
    if result.returncode == 0:
        assert f"{count} passed" in text, text
    return result.returncode, text


def failing_contract_probe(label, owner):
    log = logs / f"{label}-accepted.txt"
    with log.open("w") as output:
        result = subprocess.run(
            ["node", str(probe), owner],
            cwd=root, env=os.environ, stdout=output, stderr=subprocess.STDOUT, timeout=900)
    text = log.read_text()
    assert result.returncode == 1 and owner in text and "function output mismatch" in text, text
    assert "runtime probes did not execute" not in text and "error[E" not in text, text
    print(f"{label}: actual accepted-contract output observation also fails", flush=True)


code, text = run("baseline", "accepted_iterator_", 5)
assert code == 0, text
for label, filename, before, after, test, assertion in faults:
    path = source / filename
    expected = original[path]
    assert path.read_bytes() == expected
    assert expected.count(before) == 1, (filename, before)
    assert before.count(b"\n") == after.count(b"\n"), "body faults must preserve compiler spans"
    try:
        path.write_bytes(expected.replace(before, after, 1))
        code, text = run(label, test, 1)
        assert code == 101 and "1 failed" in text and "panicked at" in text, text
        assert "error[E" not in text and assertion in text, text
        print(f"{label}: compiled named native assertion failed", flush=True)
        if label == "collection":
            failing_contract_probe(label, "safe_cex_exchange::js_runtime::collections::Collection")
        elif filename == "value.rs":
            failing_contract_probe(label, "safe_cex_exchange::js_runtime::value::Js")
    finally:
        path.write_bytes(expected)
    code, text = run(f"{label}-restored", test, 1)
    assert code == 0 and path.read_bytes() == expected, text
    print(f"{label}: exact bytes restored, native witness passes", flush=True)
assert all(path.read_bytes() == content for path, content in original.items())
with (logs / "accepted-restored.txt").open("w") as output:
    result = subprocess.run(["node", str(probe), "all"], cwd=root, env=os.environ,
                            stdout=output, stderr=subprocess.STDOUT, timeout=900)
assert result.returncode == 0, (logs / "accepted-restored.txt").read_text()
print("All five owners and all three new Js entrypoints have compiled controls.", flush=True)
