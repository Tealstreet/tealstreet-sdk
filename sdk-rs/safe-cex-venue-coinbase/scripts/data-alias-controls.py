import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-venue-coinbase/src/binding/value.rs"
original = source.read_bytes()
before = b"Js::Number(value) => Frame::Number(*value),"
after = b"Js::Number(_value) => Frame::Number(0.0),"
assert original.count(before) == 1
name = "typed_data_collections_keep_scalar_bits_and_separate_info_roots"
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-coinbase",
           "--lib", "data_alias_native::" + name]

def run():
    return subprocess.run(command, cwd=workspace, env={**os.environ, "CARGO_BUILD_JOBS": "1"},
                          text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

try:
    source.write_bytes(original.replace(before, after))
    failed = run()
    Path("/tmp/coinbase-data-alias-projection-red.log").write_text(failed.stdout)
    assert failed.returncode != 0 and "panicked at" in failed.stdout, failed.stdout
    assert name + " ... FAILED" in failed.stdout, failed.stdout
finally:
    source.write_bytes(original)

assert source.read_bytes() == original
restored = run()
Path("/tmp/coinbase-data-alias-projection-restored.log").write_text(restored.stdout)
assert restored.returncode == 0 and "1 passed; 0 failed" in restored.stdout, restored.stdout
print("Actual native number projection: compiled assertion RED; exact source restoration GREEN")
