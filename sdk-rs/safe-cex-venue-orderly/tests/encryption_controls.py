"""One production mutation per encryption row; one Node subprocess per corpus."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH"), "SAFE_CEX_TYPESCRIPT_PATH required"
command = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--manifest-path", str(crate.parent / "Cargo.toml"),
           "-p", "safe-cex-venue-orderly", "--test", "encryption_parity", "--jobs", "1", "--locked"]
def run():
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
controls = [
    ("Sha512::digest(message)", "Sha512::digest(&message[..0])", "sha512ForEd25519/digest/1"),
    ("""value.replace('+', "-")""", """value.replacen('+', "-", 1)""", "base64url/text/3"),
    ("Ok(base64url(&STANDARD.encode(signature)))", "Ok(base64url(&STANDARD.encode(signature)).trim_end_matches('=').to_owned())", "orderlySign/signature/0"),
]
baseline = run()
assert baseline.returncode == 0, baseline.stdout
file = crate / "src/encryption.rs"
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
print("3 production mutations rejected; 3 restorations passed")
