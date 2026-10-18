import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-venue-coinbase/src/portfolios_balances.rs"
original = source.read_bytes()
needle = b'let r#type = text("type")?;'
assert original.count(needle) == 1
command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-coinbase",
           "--lib", "portfolio_alias_native::portfolio_and_supported_type_aliases_keep_live_roots_and_native_metadata"]
environment = {**os.environ, "CARGO_BUILD_JOBS": "1"}

def run():
    return subprocess.run(command, cwd=workspace, env=environment, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

try:
    source.write_bytes(original.replace(needle, needle + b"\n        metadata.clear();"))
    failed = run()
    Path("/tmp/coinbase-portfolio-alias-control-red.log").write_text(failed.stdout)
    assert failed.returncode != 0 and "panicked at" in failed.stdout
    assert "portfolio_and_supported_type_aliases_keep_live_roots_and_native_metadata ... FAILED" in failed.stdout
finally:
    source.write_bytes(original)

assert source.read_bytes() == original
restored = run()
Path("/tmp/coinbase-portfolio-alias-control-restored.log").write_text(restored.stdout)
assert restored.returncode == 0 and "1 passed; 0 failed" in restored.stdout, restored.stdout
print("Production portfolio metadata removal: compiled native assertion RED; exact source restoration GREEN")
