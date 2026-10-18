"""Compile independent real-binding faults, restore exact bytes, replay GREEN."""
from pathlib import Path
import subprocess

sdk = Path(__file__).resolve().parents[2]
logs = sdk / "target/okx-divergence-controls"
logs.mkdir(parents=True, exist_ok=True)
base = ["cargo", "test", "--locked", "--jobs", "2", "-p", "safe-cex-venue-okx", "--lib"]
owner = "binding::exchange::divergence_binding::"


def run(test, log):
    result = subprocess.run(base + [owner + test, "--", "--exact"], cwd=sdk,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors="replace")
    (logs / log).write_text(output)
    assert "running 1 test" in output, output
    return result.returncode, output


placement = sdk / "safe-cex-venue-okx/src/rest_operations.rs"
sockets = sdk / "safe-cex-venue-okx/src/binding/sockets.rs"
lifecycle = sdk / "safe-cex-venue-okx/src/family_lifecycle.rs"
conversion = "batch_ids.push(js_string(id).map_err(coercion)?);"
controls = []
for algo, label in [(False, "normal"), (True, "algo")]:
    condition = "algo" if algo else "!algo"
    fault = f'''if {condition} && id.and_then(Value::as_str).is_none() {{
                                return Err(ExchangeError::new("OKX order id must be a string"));
                            }}
                            {conversion}'''
    controls.append((label, placement, conversion, fault,
                     f"real_{label}_placement_keeps_numeric_live_order_id",
                     "live placement ID must survive"))
controls.extend([
    ("parse", sockets, "finite::parse_finite(value.as_ref())", "finite::parse_finite(value.as_ref().filter(|_| false))",
     "real_private_finite_members_keep_wire_values_and_bits", "OKX real parseFinite wire"),
    ("sum", sockets, "finite::sum_finite(&values)", "finite::sum_finite(&values[..0])",
     "real_private_finite_members_keep_wire_values_and_bits", "OKX real sumFinite wire"),
    ("constructor", lifecycle, 'assign(h, "region", region.clone())?;',
     'assign(h, "region", (h.value.literal)("okxeu"))?;',
     "real_constructor_preserves_typed_options_and_socket_receivers", "assertion failed"),
])

for label, source, old, fault, test, marker in controls:
    original = source.read_bytes()
    text = original.decode()
    assert text.count(old) == 1, label
    code, output = run(test, f"{label}-baseline.log")
    assert code == 0 and "1 passed" in output, output
    try:
        source.write_text(text.replace(old, fault))
        code, output = run(test, f"{label}-red.log")
        assert code != 0 and "test result: FAILED" in output and marker in output, output
        print(f"{label}: compiled named RED", flush=True)
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    code, output = run(test, f"{label}-restored-green.log")
    assert code == 0 and "1 passed" in output, output
    print(f"{label}: exact restoration GREEN", flush=True)
