"""Require a compiled missing-Get fault in the real Backpack REST array host."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-venue-backpack/src/adapter.rs"
test = "native_rest_array_host_retains_owner_and_inherited_getter_completion"


def run(label, red=False):
    if shutil.disk_usage("/").free < 15 * 1024**3:
        subprocess.run(["cargo", "clean"], cwd=root, check=True)
    result = subprocess.run(
        ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-backpack",
         "--test", "adapter_classes", test, "--", "--exact"],
        cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    Path("/tmp/js-array-backpack-" + label + ".log").write_text(result.stdout)
    if red:
        assert result.returncode != 0 and "REST_ARRAY inherited getter reached" in result.stdout, result.stdout
        assert "1 failed" in result.stdout, result.stdout
    else:
        assert result.returncode == 0 and "1 passed" in result.stdout, result.stdout
    print(label, "compiled named RED" if red else "GREEN", flush=True)


original = source.read_bytes()
needle = """        if a.has_index(index) {
            Ok(Some(a.get(&index.to_string())?))
        } else {
            Ok(None)
        }"""
assert original.decode().count(needle) == 1, "Backpack array Get anchor drift"
run("baseline")
try:
    source.write_text(original.decode().replace(needle, "        let _ = (a, index);\n        Ok(None)"))
    run("missing-get", True)
finally:
    source.write_bytes(original)
run("restored")
