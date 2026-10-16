import argparse
from pathlib import Path
import re
import json
import os
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("mode", choices=["consuming-empty", "extra", "extra-without-guards", "consuming-empty-without-guards", "kv-extras"])
args = parser.parse_args()
crate = Path(__file__).resolve().parents[1]
source = crate / "src/types.rs"
tests = [crate / "tests/field_identity.rs", crate / "tests/numeric_ingress.rs"]
backend = crate / "src/backends.rs"
paths = [source, backend, *tests]
original = {path: path.read_bytes() for path in paths}
needle = "        ChunkRecordWire::deserialize(serde_json::Value::Object(object))\n"
assert source.read_text().count(needle) == 1, "chunk decoder control source drift"
consume_empty = args.mode.startswith("consuming-empty")
operation = "record.candles.clear();" if consume_empty else "if let Some(extra) = record.candles.first().cloned() { record.candles.push(extra); }"
try:
    if args.mode == "kv-extras":
        needle = "            Ok(all(futures, runtime).await?.into_iter().flatten().collect())"
        assert backend.read_text().count(needle) == 1, "KV collection control drift"
        replacement = """            let mut chunks: Chunks = all(futures, runtime).await?.into_iter().flatten().collect();
            match std::env::var("P3B_CARDINALITY_KIND").as_deref() {
                Ok("chunk") => { let extra = chunks[&0].clone(); drop(chunks.insert(1, extra)); }
                Ok("width") => chunks.get_mut(&0).unwrap().push(None),
                Ok("occupied") => { let sparse = chunks.get_mut(&0).unwrap(); let extra = sparse.iter().flatten().next().cloned().unwrap(); sparse[3] = Some(extra); }
                _ => {}
            }
            Ok(chunks)"""
        backend.write_text(backend.read_text().replace(needle, replacement))
        build = subprocess.run(["cargo", "test", "--manifest-path", str(crate.parent / "Cargo.toml"), "-p", "safe-cex-candle-cache", "--test", "field_identity", "--no-run", "--message-format=json", "--jobs", "1"], capture_output=True, text=True)
        assert build.returncode == 0, build.stdout + build.stderr
        artifacts = [json.loads(line) for line in build.stdout.splitlines()]
        executables = [item["executable"] for item in artifacts if item.get("reason") == "compiler-artifact" and item["target"]["name"] == "field_identity" and item.get("executable")]
        assert len(executables) == 1, "KV control must bind one compiler artifact"
        for kind, assertion in [("chunk", "CACHE-CARDINALITY native chunk count"), ("width", "CACHE-CARDINALITY sparse width"), ("occupied", "CACHE-CARDINALITY occupied candles")]:
            env = dict(os.environ, P3B_CARDINALITY_KIND=kind)
            run = subprocess.run([executables[0], "--exact", "kv_consumer_native_field_identity"], env=env, capture_output=True, text=True)
            assert run.returncode == 101 and assertion in run.stdout + run.stderr, run.stdout + run.stderr
            print(f"KV extra {kind}: compiled, 101 at {assertion}")
    else:
        source.write_text(source.read_text().replace(needle, needle + "            .map(|mut record| { " + operation + " record })\n"))

        if args.mode.endswith("without-guards"):
            for path in tests:
                text, count = re.subn(r'\s*assert_eq!\(\s*record\.candles\.len\(\),\s*1,\s*"CACHE-CARDINALITY [^"]+"\s*\);', "", path.read_text())
                assert count == 1, f"length control drift: {path.name}"
                path.write_text(text)
        run = subprocess.run(["cargo", "test", "--manifest-path", str(crate.parent / "Cargo.toml"), "-p", "safe-cex-candle-cache", "--test", "field_identity", "--test", "numeric_ingress", "--no-fail-fast", "--jobs", "1"], capture_output=True, text=True)
        output = run.stdout + run.stderr
        assert "Finished `test`" in output, f"mutation did not compile:\n{output}"
        if args.mode == "extra-without-guards":
            assert run.returncode == 0, output
            assert "2 passed; 0 failed" in output and "1 passed; 0 failed" in output, output
            print("extra guard removal: 3/0/0, original index walks ignore extra native candles")
        elif args.mode == "consuming-empty-without-guards":
            assert run.returncode == 101 and "index out of bounds" in output, output
            print("consuming empty without guards: 101 at native indexing, not trailing-input rejection")
        else:
            assert run.returncode == 101, output
            for assertion in ["CACHE-CARDINALITY decoded candles", "CACHE-CARDINALITY numeric ingress candles"]:
                assert assertion in output, f"missing named assertion {assertion}:\n{output}"
            print(f"{args.mode}: compiled, 101 at both native candle cardinality assertions")
finally:
    for path, content in original.items():
        path.write_bytes(content)
