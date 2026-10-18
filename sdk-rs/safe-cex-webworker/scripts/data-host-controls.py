"""Compile browser DATA faults and test actual WASM -> JS -> structuredClone."""
from pathlib import Path
import os
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-webworker/src/data_post/browser.rs"
original = source.read_bytes()
output = Path("/tmp") / ("worker-data-host-controls-" + str(os.getpid()))
bindgen = shutil.which("wasm-bindgen") or str(Path.home() / ".cargo/bin/wasm-bindgen")

def command(args, label):
    result = subprocess.run(args, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/worker-data-host-" + label + ".log").write_text(result.stdout)
    return result

def run(label, failure=None):
    subprocess.run(["df", "-h", "/"], check=True)
    if shutil.disk_usage("/").free < 15 * 1024**3:
        subprocess.run(["cargo", "clean", "--target-dir", str(root / "target")], cwd=root, check=True)
    build = command(["cargo", "build", "--locked", "-p", "safe-cex-webworker", "--example", "worker_data_host", "--target", "wasm32-unknown-unknown", "--jobs", "1", "--target-dir", str(root / "target")], label + "-build")
    assert build.returncode == 0, build.stdout
    module = command([bindgen, str(root / "target/wasm32-unknown-unknown/debug/examples/worker_data_host.wasm"), "--target", "nodejs", "--out-dir", str(output)], label + "-bindgen")
    assert module.returncode == 0, module.stdout
    witness = command(["node", str(root / "safe-cex-webworker/tests/worker-data-host.cjs"), str(output / "worker_data_host.js")], label + "-witness")
    if failure:
        assert witness.returncode != 0 and "ERR_ASSERTION" in witness.stdout and failure in witness.stdout, witness.stdout
    else:
        assert witness.returncode == 0 and "host failure PASS" in witness.stdout, witness.stdout
    print(label, "named failure" if failure else "passed", flush=True)

s = original.decode()
controls = [
    ("undefined-null", "Js::Undefined => Ok(JsValue::UNDEFINED)", "Js::Undefined => Ok(JsValue::NULL)", "WORKER-DATA own undefined"),
    ("nonfinite-null", "Js::Number(value) => Ok(JsValue::from_f64(*value))", "Js::Number(value) => Ok(if value.is_finite() {JsValue::from_f64(*value)} else {JsValue::NULL})", "WORKER-DATA native NaN"),
    ("negative-zero", "Js::Number(value) => Ok(JsValue::from_f64(*value))", "Js::Number(value) => Ok(JsValue::from_f64(if *value == 0.0 {0.0} else {*value}))", "WORKER-DATA native signed zero"),
]
run("baseline")
for label, needle, replacement, failure in controls:
    assert s.count(needle) == 1
    try:
        source.write_text(s.replace(needle, replacement))
        run(label, failure)
    finally:
        source.write_bytes(original)
    run(label + "-restored")
