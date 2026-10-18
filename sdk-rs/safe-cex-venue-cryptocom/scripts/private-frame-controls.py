"""Prove real private-socket topic delivery detects a disconnected production handler.

Run alone: each mutation changes socket_handlers.rs, requires a compiled named
assertion failure, then restores exact bytes and requires the target to pass.
"""
from pathlib import Path
import os
import subprocess
import sys

SDK = Path(__file__).resolve().parents[2]
SOURCE = SDK / "safe-cex-venue-cryptocom/src/socket_handlers.rs"
CONTROLS = [
    ("orders", 'Some("handleOrderUpdate")', "private_order_frames_create_partially_fill_close_and_cancel_orders"),
    ("positions", 'Some("handlePositionUpdate")', "private_position_frames_update_hedged_sides_and_remove_zero_quantity"),
    ("balance", 'Some("handleBalanceUpdate")', "private_balance_frames_replace_rest_currency_shape_and_ignore_empty_updates"),
]


def run(test):
    return subprocess.run(
        ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-cryptocom",
         "--test", "exchange_binding", test, "--", "--exact"],
        cwd=SDK, env=os.environ.copy(), capture_output=True, text=True,
    )


def main():
    selected = set(sys.argv[1:])
    if selected - {name for name, _, _ in CONTROLS}:
        raise SystemExit("unknown control")
    original = SOURCE.read_bytes()
    for name, before, test in CONTROLS:
        if selected and name not in selected:
            continue
        assert original.count(before.encode()) == 1, "control must target one production dispatch"
        baseline = run(test)
        assert baseline.returncode == 0 and "1 passed" in baseline.stdout, baseline.stdout + baseline.stderr
        try:
            SOURCE.write_bytes(original.replace(before.encode(), b"None", 1))
            mutant = run(test)
            output = mutant.stdout + mutant.stderr
            assert mutant.returncode == 101 and f"{test} ... FAILED" in output and "test result: FAILED" in output, output
            print(f"{name}: compiling production dispatch mutant failed named runtime assertion", flush=True)
        finally:
            SOURCE.write_bytes(original)
        restored = run(test)
        assert restored.returncode == 0 and "1 passed" in restored.stdout, restored.stdout + restored.stderr
        print(f"{name}: byte-restored production passes", flush=True)


if __name__ == "__main__":
    main()
