"""One production mutation per margin helper row; one Node subprocess per corpus."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH"), "SAFE_CEX_TYPESCRIPT_PATH required"
command = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--manifest-path", str(crate.parent / "Cargo.toml"),
           "-p", "safe-cex-venue-orderly", "--test", "margin_parity", "--jobs", "1", "--locked"]
def run():
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
controls = [
    ('format!("{base_id}:isolated")', 'format!("isolated:{base_id}")', 'position-id/source/0'),
]

baseline = run()
assert baseline.returncode == 0, baseline.stdout
file = crate / "src/margin.rs"
for old, new, assertion in controls:
    saved = file.read_bytes()
    try:
        source = saved.decode()
        assert source.count(old) == 1, old
        file.write_text(source.replace(old, new, 1))
        failed = run()
        assert failed.returncode not in (0, 126) and "panicked at" in failed.stdout and assertion in failed.stdout, failed.stdout
        print("FAIL observed: " + assertion, flush=True)
    finally:
        file.write_bytes(saved)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    print("PASS restored: " + assertion, flush=True)
print("1 production mutation rejected; 1 restoration passed")
