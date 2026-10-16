"""Verify the MEXC leverage oracle detects independent Rust and TS mutations."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
root = crate.parents[1]
rust_file = crate / "src/exchange.rs"
original = rust_file.read_text()
command = [
    "cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml",
    "-p", "safe-cex-venue-mexc", "--lib", "--jobs", "2", "--",
    "--exact", "exchange::tests::mexc_leverage_override_dispatches_through_dyn_exchange",
    "--nocapture",
]
env = os.environ.copy()
env["SAFE_CEX_TYPESCRIPT_PATH"] = str(crate / "target/oracle-deps/node_modules/typescript/lib/typescript.js")
env["PATH"] = "/opt/homebrew/opt/rustup/bin:" + env.get("PATH", "")

try:
    anchor = "let fetched = FetchedLeverage {\n                                    buy_leverage: buy,"
    assert original.count(anchor) == 1
    rust_file.write_text(original.replace(anchor, "let fetched = FetchedLeverage {\n                                    buy_leverage: buy + 1.0,"))
    result = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 101, result.stdout
    assert "mexc_leverage_override_dispatches_through_dyn_exchange ... FAILED" in result.stdout, result.stdout
    assert "override buy leverage" in result.stdout, result.stdout
    print("Rust leverage mutation: named assertion failure, exit 101")
finally:
    rust_file.write_text(original)

anchor = "self.fetch_leverage_for_symbol(symbol).await?;"
assert original.count(anchor) == 1
try:
    rust_file.write_text(original.replace(anchor, "let _ = symbol;"))
    result = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 101, result.stdout
    assert "mexc_leverage_override_dispatches_through_dyn_exchange ... FAILED" in result.stdout, result.stdout
    assert "metadata update delegates to leverage fetch" in result.stdout, result.stdout
    print("Rust metadata mutation: named assertion failure, exit 101")
finally:
    rust_file.write_text(original)

env["MEXC_LEVERAGE_TS_CONTROL"] = "buy-leverage"
result = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert result.returncode == 101, result.stdout
assert "mexc_leverage_override_dispatches_through_dyn_exchange ... FAILED" in result.stdout, result.stdout
assert "TypeScript leverage buy" in result.stdout, result.stdout
print("TypeScript leverage mutation: named assertion failure, exit 101")

env["MEXC_LEVERAGE_TS_CONTROL"] = "metadata"
result = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert result.returncode == 101, result.stdout
assert "mexc_leverage_override_dispatches_through_dyn_exchange ... FAILED" in result.stdout, result.stdout
assert "TypeScript metadata delegation" in result.stdout, result.stdout
print("TypeScript metadata mutation: named assertion failure, exit 101")

env.pop("MEXC_LEVERAGE_TS_CONTROL")
subprocess.run(command, cwd=root, env=env, check=True)
