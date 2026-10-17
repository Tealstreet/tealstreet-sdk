"""Compile the two real-session regressions against the pre-fix production host."""
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCES = [
    ROOT / "sdk-rs/safe-cex-exchange/src/websocket.rs",
    ROOT / "sdk-rs/safe-cex-exchange/src/js_runtime/sockets.rs",
]
BASE = "ad92e6a798"
COMMAND = [
    "cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "-p",
    "safe-cex-venue-kucoin", "--lib", "native_reconnect_tests::typed_",
]
fixed = {path: path.read_bytes() for path in SOURCES}
try:
    for path in SOURCES:
        path.write_bytes(subprocess.check_output([
            "git", "show", f"{BASE}:{path.relative_to(ROOT)}",
        ], cwd=ROOT))
    result = subprocess.run(COMMAND, cwd=ROOT, capture_output=True, text=True)
    output = result.stdout + result.stderr
    sys.stdout.write(output)
    sys.stdout.flush()
    assert result.returncode == 101 and "could not compile" not in output
    for kind in ["public", "private"]:
        assert f"typed_{kind}_reconnect_renews_kucoin_session ... FAILED" in output
    assert "typed reconnect must run the source token reset before awaiting HTTP" in output
finally:
    for path, original in fixed.items():
        path.write_bytes(original)
restored = subprocess.run(COMMAND, cwd=ROOT)
assert restored.returncode == 0, "restored real-session regressions failed"
