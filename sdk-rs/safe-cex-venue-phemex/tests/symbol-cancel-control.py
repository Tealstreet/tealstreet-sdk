"""Prove the symbol-cancel witness observes the production filter, then restore it."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-phemex/src/binding/methods.rs"
original = source.read_bytes()
before = 'if o.get("symbol")?.strict_equals(&s) {'
after = 'if !o.get("symbol")?.strict_equals(&s) {'
assert original.decode().count(before) == 1
command = [
    "cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml",
    "-p", "safe-cex-venue-phemex", "--lib",
    "symbol_cancel_uses_the_live_store_filter_and_real_cancellation_client",
    "--locked", "--jobs", "1",
]

try:
    source.write_text(original.decode().replace(before, after))
    broken = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    assert broken.returncode == 101, broken.stdout
    assert "test result: FAILED" in broken.stdout, broken.stdout
    assert "one real matching-symbol batch" in broken.stdout, broken.stdout
    print("Production symbol-filter mutation: compiled, named binding assertion failed (101)", flush=True)
finally:
    source.write_bytes(original)

restored = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT)
assert restored.returncode == 0, restored.stdout
assert source.read_bytes() == original
print("Exact production restoration: binding witness passed (0)")
