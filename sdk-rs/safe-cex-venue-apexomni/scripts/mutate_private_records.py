"""Symmetric production serde-name swaps: one per row, one compiled corpus.

The normal roundtrip survives these swaps. Decoded native field assertions must
fail for all eight rows before sources are restored and the same corpus passes.
"""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-apexomni/src/private_records.rs"
original = source.read_text()
pairs = {
    "WalletObject": ("userId", "accountId"),
    "OpenPositionObject": ("userId", "accountId"),
    "AccountsItem": ("takerFeeRate", "makerFeeRate"),
    "AccountObject": ("starkKey", "positionId"),
    "WorstPriceObject": ("worstPrice", "bidOnePrice"),
    "FundingRateObject": ("id", "symbol"),
    "HistoricalPNLObject": ("symbol", "size"),
    "AccountBalanceObject": ("totalEquityValue", "availableBalance"),
}
mutated = original
for name, (left, right) in pairs.items():
    start = mutated.index("// ts: " + name + "\n")
    end = mutated.find("// ts: ", start + 1)
    if end == -1:
        end = len(mutated)
    block = mutated[start:end]
    a, b = f'rename = "{left}"', f'rename = "{right}"'
    assert block.count(a) == block.count(b) == 2, name
    changed = block.replace(a, "RENAME_PLACEHOLDER").replace(b, a).replace("RENAME_PLACEHOLDER", b)
    mutated = mutated[:start] + changed + mutated[end:]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH"), "SAFE_CEX_TYPESCRIPT_PATH required"
cmd = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--locked", "--manifest-path", str(root / "sdk-rs/Cargo.toml"), "-p", "safe-cex-venue-apexomni", "--test", "records", "--jobs", "1", "private_", "--", "--test-threads=1", "--nocapture"]
logs = root / "sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True, exist_ok=True)
try:
    source.write_text(mutated)
    result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / "private-row-mutations.log").write_text(output)
    assert result.returncode == 101 and "test result: FAILED" in output, output
    # Every native assertion is preceded by a successful structural roundtrip.
    # Exact serialized property-order checks may also catch the swaps.
    for name, (left, _) in pairs.items():
        assert f"{name}.{left} decoded native field" in output, output
    print("8 symmetric serde swaps: structural roundtrip passed; all 8 named native assertions failed at exit 101", flush=True)
finally:
    source.write_text(original)
result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
(logs / "private-row-restored.log").write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print("Restored private corpus passed; 2 sequential Cargo invocations", flush=True)
