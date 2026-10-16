"""Sequential compiling controls for legacy order mapping; restores each mutation."""
from pathlib import Path
import os
import subprocess
import sys

path = Path("sdk-rs/safe-cex-venue-woo/src/orders.rs")
source = path.read_text()
changes = [
    ("numeric-zero", "safe_cex_num::js_number(value).map_err", "safe_cex_num::js_number(value).map(|n| if n == 0.0 { 0.0 } else { n }).map_err"),
    ("numeric-overflow", "safe_cex_num::js_number(value).map_err", "safe_cex_num::js_number(value).map(|n| if n.is_finite() { n } else { 0.0 }).map_err"),
    ("truthy-fallback", "if js_truthy(first) { first } else { second }", "if first.is_some() { first } else { second }"),
    ("algo-filter", 'if !js_truthy(trigger_price) && algo_type.and_then(Value::as_str) != Some("TRAILING_STOP") {', 'if false && !js_truthy(trigger_price) && algo_type.and_then(Value::as_str) != Some("TRAILING_STOP") {'),
    ("trigger-branch", "sl_trigger_type: if stop { trigger_type } else { None }", "sl_trigger_type: if !stop { trigger_type } else { None }"),
    ("parent-info", "info: Some(Value::Object(row.clone())),\n            sl_trigger_type", "info: Some(Value::Object(child.clone())),\n            sl_trigger_type"),
    ("parent-id", 'field(row, "rootAlgoOrderId")?,', 'None,'),
    ("clock", "last_update: now(),", "last_update: { let _ = now(); 0.0 },"),
    ("inherited-identity", '"typed Order.{name} refuses inherited {}"', '"typed Order.{name} cannot carry undefined {}"'),
]
env = dict(os.environ)
env["PATH"] = "/opt/homebrew/opt/rustup/bin:" + env["PATH"]
for name, before, after in changes:
    if sys.argv[1:] and name not in sys.argv[1:]:
        continue
    assert before in source, name
    try:
        path.write_text(source.replace(before, after, 1))
        result = subprocess.run([
            "cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml",
            "-p", "safe-cex-venue-woo", "--locked", "--offline", "--jobs", "2",
            "--test", "orders",
        ], env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        Path(f"/tmp/woo-orders-{name}.log").write_text(output)
        assert result.returncode == 101 and "test result: FAILED" in output and any(marker in output for marker in ("assertion `left == right` failed", "source mapping succeeds:", "preserve inherited identity")), (name, output)
        print(name, "assertion-red 101", flush=True)
    finally:
        path.write_text(source)
