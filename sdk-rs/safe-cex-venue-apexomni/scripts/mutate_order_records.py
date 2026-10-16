"""Five row mutations in one production corpus; native axes, restore, pass."""
from pathlib import Path
import os
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-apexomni/src/order_records.rs"
original = source.read_text()
mutated = original
assert mutated.count("Ok(Iso8601::String(value))") == 1
mutated = mutated.replace("Ok(Iso8601::String(value))", 'Ok(Iso8601::String(value + "_MUTATED"))')
start = mutated.index("// ts: WithPrice\n")
end = mutated.index("// ts: OrderWithClientId\n")
block = mutated[start:end]
assert block.count("human_price: fields.human_price,") == 1
changed = block.replace("human_price: fields.human_price,", 'human_price: fields.human_price + "_MUTATED",')
assert changed.count("#[serde(skip_serializing)]") == 1
changed = changed.replace('#[serde(skip_serializing)]', '#[serde(skip_serializing_if = "Option::is_none")]')
mutated = mutated[:start] + changed + mutated[end:]
for name, left, right in [("OrderWithClientId","positionId","humanSize"),("CreateOrderOptionsObject","clientOrderId","positionId"),("OrderResponseObject","id","accountId")]:
    start = mutated.index("// ts: " + name + "\n")
    end = mutated.find("// ts: ",start+1)
    if end == -1: end = len(mutated)
    block = mutated[start:end]
    a, b = f'rename = "{left}"', f'rename = "{right}"'
    assert block.count(a) == block.count(b) == 2, name
    changed = block.replace(a,"RENAME_PLACEHOLDER").replace(b,a).replace("RENAME_PLACEHOLDER",b)
    mutated = mutated[:start] + changed + mutated[end:]
labels = ["ISO8601 independent native axis", "WithPrice.humanPrice independent native axis", "OrderWithClientId.positionId independent native axis", "CreateOrderOptionsObject.clientOrderId independent native axis", "OrderResponseObject.id independent native axis", "WithPrice present undefined property omitted"]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH")
cmd = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--locked", "--manifest-path", str(root / "sdk-rs/Cargo.toml"), "-p", "safe-cex-venue-apexomni", "--test", "records", "--jobs", "1", "order_row_controls_", "--", "--test-threads=1", "--nocapture"]
logs = root / "sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True, exist_ok=True)
try:
    source.write_text(mutated)
    result = subprocess.run(cmd,cwd=root,capture_output=True,text=True)
    output = result.stdout + result.stderr
    (logs / "order-row-mutations.log").write_text(output)
    assert result.returncode == 101 and "test result: FAILED" in output, output
    for label in labels: assert label in output, output
    print("5 row mutations plus undefined-omission mutation: 6 named assertion failures at exit 101",flush=True)
finally:
    source.write_text(original)
result = subprocess.run(cmd,cwd=root,capture_output=True,text=True)
(logs / "order-row-restored.log").write_text(result.stdout+result.stderr)
assert result.returncode == 0,result.stdout+result.stderr
print("Restored 5 native axes and undefined-omission control passed; two sequential Cargo invocations",flush=True)
