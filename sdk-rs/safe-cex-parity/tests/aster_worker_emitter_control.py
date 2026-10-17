from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-webworker/src/exchange_worker_start.rs"
original = source.read_bytes()
needle = b'base.on(\n        "fill",'
assert original.count(needle) == 1
mutated = original.replace(needle, b'base.on(\n        "disconnected-fill",')


def test(name):
    result = subprocess.run(
        ["cargo", "+1.99.0", "test", "--locked", "--jobs=1", "-p", "safe-cex-parity",
         "--test", "aster_worker_emitter", name], cwd=workspace,
        env={**os.environ, "CARGO_INCREMENTAL": "0"}, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900,
    )
    print(result.stdout, flush=True)
    return result


try:
    source.write_bytes(mutated)
    result = test("native_private_order_and_fill_have_source_event_arguments")
    assert result.returncode == 101 and "could not compile" not in result.stdout
    assert "native_private_order_and_fill_have_source_event_arguments ... FAILED" in result.stdout
    assert "left: 0" in result.stdout and "right: 1" in result.stdout
finally:
    assert source.read_bytes() == mutated, "refusing to overwrite concurrent source edits"
    source.write_bytes(original)
restored = test("")
assert restored.returncode == 0 and "2 passed" in restored.stdout
assert source.read_bytes() == original
print("Compiled real worker-fill disconnect failed; exact restoration passed both master-ABI cross-layer witnesses.")
