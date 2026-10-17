"""Delete each cancelByCloid classifier arm independently; restore exact bytes."""
from pathlib import Path
import re
import signal
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / "src/rate_limit.rs"
baseline = source.read_bytes()
text = baseline.decode()
needle = 'Some("cancel" | "cancelByCloid")'
positions = [match.start() for match in re.finditer(re.escape(needle), text)]
assert len(positions) == 2
command = ["cargo", "test", "-p", "safe-cex-venue-hyperliquid", "--test", "rate_encoded",
           "cancel_by_cloid_weight_reaches_real_http_prepare_for_raw_and_value_bodies", "--", "--exact"]

def interrupted(_signal, _frame):
    raise KeyboardInterrupt("restoring production bytes")
signal.signal(signal.SIGTERM, interrupted)
for kind, offset in zip(["value", "raw"], positions):
    try:
        source.write_text(text[:offset] + text[offset:].replace(needle, 'Some("cancel")', 1))
        mutant = subprocess.run(command, cwd=crate.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path(f"/tmp/hl-cloid-{kind}-mutant.log").write_text(mutant.stdout)
        assert mutant.returncode == 101 and "test result: FAILED" in mutant.stdout
        assert "cancelByCloid batch pacing at HTTP prepare" in mutant.stdout
        assert "could not compile" not in mutant.stdout
    finally:
        source.write_bytes(baseline)
    restored = subprocess.run(command, cwd=crate.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path(f"/tmp/hl-cloid-{kind}-restored.log").write_text(restored.stdout)
    assert restored.returncode == 0 and "test result: ok" in restored.stdout
    assert source.read_bytes() == baseline
    print(f"{kind}: compiled HTTP pacing failure; exact restoration passed", flush=True)
