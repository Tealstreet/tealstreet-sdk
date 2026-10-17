from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-candle-cache/src/backends.rs"
original = source.read_bytes()
needle = b'''                        let Some(candle) = crate::codec::decode_json_candle(candle) else {
                            continue;
                        };'''
assert original.count(needle) == 1
mutated = original.replace(needle, needle.replace(b"continue;", b'return Err("injected malformed cache row error".to_owned());'))


def test():
    result = subprocess.run(
        ["cargo", "+1.99.0", "test", "--locked", "--jobs=1", "-p", "safe-cex-candle-cache",
         "--lib", "malformed_cached_candle_is_a_miss_and_refetches_without_error"],
        cwd=workspace, env={**os.environ, "CARGO_INCREMENTAL": "0"}, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900,
    )
    print(result.stdout, flush=True)
    return result


try:
    source.write_bytes(mutated)
    control = test()
    assert control.returncode == 101 and "could not compile" not in control.stdout
    assert "malformed_cached_candle_is_a_miss_and_refetches_without_error ... FAILED" in control.stdout
    assert "CACHE-MALFORMED unsupported cache rows must not fail the candle request" in control.stdout
finally:
    assert source.read_bytes() == mutated, "refusing to overwrite concurrent source edits"
    source.write_bytes(original)
restored = test()
assert restored.returncode == 0 and "1 passed" in restored.stdout
assert source.read_bytes() == original
print("Malformed-row error propagation compiled and failed; exact restoration passed all 38 structural miss/refetch cases.")
