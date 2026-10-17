from pathlib import Path
import os
import subprocess
import tempfile

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-webworker/src/data_post/browser.rs"
original = source.read_bytes()
custom = b'''            } else {
                for (key, child) in value.own_entries().map_err(|e| e.to_exchange_error())? {
                    property(&object, &string(&key), &materialize(&child, seen)?, true)?;
                }
            }'''
custom_mutant = b'''            }
            for (key, child) in value.own_entries().map_err(|e| e.to_exchange_error())? {
                property(&object, &string(&key), &materialize(&child, seen)?, true)?;
            }'''
assert original.count(custom) == 1
assert original.count(b'if key == "cause"') == 1


def execute(output):
    commands = [
        ["cargo", "+1.99.0", "build", "--locked", "--jobs=1", "-p", "safe-cex-webworker",
         "--example", "worker_data_host", "--target", "wasm32-unknown-unknown"],
        [os.environ.get("WASM_BINDGEN", "wasm-bindgen"),
         str(workspace / "target/wasm32-unknown-unknown/debug/examples/worker_data_host.wasm"),
         "--target", "nodejs", "--out-dir", output],
    ]
    for command in commands:
        result = subprocess.run(command, cwd=workspace,
            env={**os.environ, "CARGO_INCREMENTAL": "0"}, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900)
        print(result.stdout, flush=True)
        assert result.returncode == 0, "the mutation must compile and bind actual Wasm"
    result = subprocess.run(["node", str(workspace / "safe-cex-webworker/tests/worker-data-host.cjs"),
        str(Path(output) / "worker_data_host.js")], cwd=workspace, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
    print(result.stdout, flush=True)
    return result


with tempfile.TemporaryDirectory(prefix="worker-error-clone-") as output:
    for mutant, failure in [
        (original.replace(custom, custom_mutant), "custom Error getter was read"),
        (original.replace(b'if key == "cause"', b'if key == "disabled-cause"'), "accessor Error cause was read"),
    ]:
        try:
            source.write_bytes(mutant)
            result = execute(output)
            assert result.returncode == 1 and failure in result.stdout
        finally:
            assert source.read_bytes() == mutant, "refusing to overwrite concurrent source edits"
            source.write_bytes(original)
    restored = execute(output)
    assert restored.returncode == 0 and "WORKER-ERROR actual WASM" in restored.stdout
    assert source.read_bytes() == original
print("Both compiled Error getter controls failed; exact restoration passed actual Wasm/structuredClone.")
