"""Compile named Array property loss in the real browser DATA materializer."""
from pathlib import Path
import os
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-webworker/src/data_post/browser.rs"
output = Path("/tmp") / ("worker-array-data-control-" + str(os.getpid()))
bindgen = shutil.which("wasm-bindgen") or str(Path.home() / ".cargo/bin/wasm-bindgen")

def command(args, label):
    result = subprocess.run(args, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/worker-array-data-" + label + ".log").write_text(result.stdout)
    return result

def run(label, red=False):
    subprocess.run(["df", "-h", "/"], check=True)
    if shutil.disk_usage("/").free < 15 * 1024**3:
        subprocess.run(["cargo", "clean"], cwd=root, check=True)
    build = command(["cargo", "build", "--locked", "--jobs", "1", "-p", "safe-cex-webworker", "--example", "worker_data_host", "--target", "wasm32-unknown-unknown"], label + "-build")
    assert build.returncode == 0, build.stdout
    module = command([bindgen, str(root / "target/wasm32-unknown-unknown/debug/examples/worker_data_host.wasm"), "--target", "nodejs", "--out-dir", str(output)], label + "-bindgen")
    assert module.returncode == 0, module.stdout
    witness = command(["node", str(root / "safe-cex-webworker/tests/worker-data-host.cjs"), str(output / "worker_data_host.js")], label + "-witness")
    if red:
        assert witness.returncode != 0 and "ERR_ASSERTION" in witness.stdout and "WORKER-ARRAY enumerable named property keys" in witness.stdout, witness.stdout
    else:
        assert witness.returncode == 0 and "WORKER-ARRAY actual WASM" in witness.stdout, witness.stdout
    print(label, "compiled named RED" if red else "GREEN", flush=True)

original = source.read_bytes()
needle = "for key in value.own_keys() {"
assert original.decode().count(needle) == 1, "array materializer anchor drift"
run("baseline")
try:
    source.write_text(original.decode().replace(needle, needle + '\n                if key == utf16("tag") { continue; }'))
    run("named-property-loss", True)
finally:
    source.write_bytes(original)
run("restored")
