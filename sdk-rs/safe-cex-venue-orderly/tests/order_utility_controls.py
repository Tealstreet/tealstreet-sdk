"""One production mutation per margin helper row; one Node subprocess per corpus."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH"), "SAFE_CEX_TYPESCRIPT_PATH required"
command = ["/opt/homebrew/opt/rustup/bin/cargo", "test", "--manifest-path", str(crate.parent / "Cargo.toml"),
           "-p", "safe-cex-venue-orderly", "--test", "order_utility_parity", "--jobs", "1", "--locked"]
def run():
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
controls = [
    ('src/utils.rs', 'order_type == OrderType::TrailingStopLoss', 'order_type == OrderType::Limit', 'algo/native/limit'),
    ('src/margin.rs', 'opts.side == OrderSide::Buy', 'opts.side == OrderSide::Sell', 'resolve-margin/native/buy-closes-short'),
    ('src/margin.rs', 'if selected == 1', 'if selected > 0', 'resolve-margin/error/duplicate-cached'),
    ('src/margin.rs', '            opts.symbol\n        ));', '            "BTC"\n        ));', 'resolve-margin/error/ambiguous-non-btc'),
]

baseline = run()
assert baseline.returncode == 0, baseline.stdout
for relative, old, new, assertion in controls:
    file = crate / relative
    saved = file.read_bytes()
    try:
        source = saved.decode()
        assert source.count(old) == 1, old
        file.write_text(source.replace(old, new, 1))
        failed = run()
        assert failed.returncode == 101 and "panicked at" in failed.stdout and assertion in failed.stdout, failed.stdout
        print("FAIL observed: " + assertion, flush=True)
    finally:
        file.write_bytes(saved)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    print("PASS restored: " + assertion, flush=True)
print("4 production mutations rejected; 4 restorations passed")
