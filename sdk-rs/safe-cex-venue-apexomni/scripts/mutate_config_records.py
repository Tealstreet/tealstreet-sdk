"""Six production row mutations checked on independent native paths, one corpus."""
from pathlib import Path
import os
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-apexomni/src/config_records.rs"
original = source.read_text()
mutated = original
pairs = {
    "MultiChainObject": ("currency", "maxWithdraw"),
    "PerpetualObject": ("usdcConfig", "usdtConfig"),
    "SymbolInfoObject": ("rankIdx", "pricePrecision"),
    "UserObject": ("ethereumAddress", "email"),
    "OrderObject": ("id", "clientId"),
}
for name, (left, right) in pairs.items():
    start = mutated.index("// ts: " + name + "\n")
    end = mutated.find("// ts: ", start + 1)
    if end == -1: end = len(mutated)
    block = mutated[start:end]
    a, b = f'rename = "{left}"', f'rename = "{right}"'
    assert block.count(a) == block.count(b) == 2, name
    changed = block.replace(a, "RENAME_PLACEHOLDER").replace(b, a).replace("RENAME_PLACEHOLDER", b)
    mutated = mutated[:start] + changed + mutated[end:]
start = mutated.index("// ts: SymbolObject\n")
end = mutated.index("// ts: PerpetualObject\n", start)
block = mutated[start:end]
assert block.count("let fields = Fields::deserialize") == 1
changed = block.replace("let fields = Fields::deserialize", "let mut fields = Fields::deserialize")
changed = changed.replace("Ok(Self {", "fields.global.fee_account_id = fields.global.fee_account_l2_key.clone();\n                Ok(Self {")
mutated = mutated[:start] + changed + mutated[end:]
labels = [
    "MultiChainObject.currency independent native axis",
    "SymbolObject.global.feeAccountId independent native axis",
    "PerpetualObject.usdcConfig.currency.id independent native axis",
    "SymbolInfoObject.rankIdx independent native axis",
    "UserObject.ethereumAddress independent native axis",
    "OrderObject.id independent native axis",
]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH")
cmd = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--locked", "--manifest-path", str(root / "sdk-rs/Cargo.toml"), "-p", "safe-cex-venue-apexomni", "--test", "records", "--jobs", "1", "config_mutation_axes_match_live_typescript", "--", "--exact", "--nocapture"]
logs = root / "sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True, exist_ok=True)
try:
    source.write_text(mutated)
    result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / "config-row-mutations.log").write_text(output)
    assert result.returncode == 101 and "test result: FAILED" in output, output
    for label in labels:
        assert f"failed: {label}" in output, output
    print("6 production row mutations: 6 independent named native assertion failures, exit 101", flush=True)
finally:
    source.write_text(original)
result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
(logs / "config-row-restored.log").write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print("Restored 6 native axes passed; two sequential Cargo invocations", flush=True)
