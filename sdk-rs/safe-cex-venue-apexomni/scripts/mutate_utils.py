"""Three independent production mutations, one Node corpus, restore and pass."""
from pathlib import Path
import os
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-apexomni/src/utils.rs"
original = source.read_text()
mutated = original
for before, after in [
    ("str_list[1].encode_utf16().count()", "str_list[1].encode_utf16().count() + 1"),
    ("apexomni-tealstreet-", "apexomni-MUTATED-"),
    ('strip_prefix("0x")', 'strip_prefix("0X")'),
]:
    assert mutated.count(before) == 1
    mutated = mutated.replace(before, after)
labels = ["getPrecision actual TypeScript result", "generateApeXRandomClientId actual TypeScript result", "dropPrefix actual TypeScript result"]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH")
cmd = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--locked", "--manifest-path", str(root / "sdk-rs/Cargo.toml"), "-p", "safe-cex-venue-apexomni", "--test", "utils", "--jobs", "1", "utility_rows_match_actual_typescript", "--", "--test-threads=1", "--nocapture"]
logs = root / "sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True, exist_ok=True)
try:
    source.write_text(mutated)
    result = subprocess.run(cmd,cwd=root,capture_output=True,text=True)
    output = result.stdout + result.stderr
    (logs / "utils-row-mutations.log").write_text(output)
    assert result.returncode == 101 and "test result: FAILED" in output, output
    for label in labels: assert label in output, output
    print("3 compiled row mutations failed their 3 named assertions at exit 101",flush=True)
finally:
    source.write_text(original)
result = subprocess.run(cmd,cwd=root,capture_output=True,text=True)
(logs / "utils-row-restored.log").write_text(result.stdout+result.stderr)
assert result.returncode == 0,result.stdout+result.stderr
print("Restored 3 utilities pass; 2 sequential Cargo invocations, 1 Node corpus each",flush=True)
