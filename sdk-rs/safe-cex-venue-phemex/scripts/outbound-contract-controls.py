"""Require compiled native failures for three reviewed outbound helper owners."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
CASES = [
    ("rest_account_control", "text_keeps_raw_symbol_units_between_utf16_affixes",
     ".chain(symbol.iter().copied())", ".chain(symbol.iter().copied().filter(|_| false))"),
    ("rest_data", "label_encodes_supplied_text_through_host",
     "h.new_text(units(s))", "h.new_text(units(s).into_iter().take(1).collect())"),
    ("websocket_data", "send_builds_one_native_frame_and_keeps_failure_identity",
     '("id", id)', '("wrongId", id)'),
]


def run(test):
    result = subprocess.run([
        "cargo", "test", "--locked", "--jobs", "1", "-q", "-p",
        "safe-cex-venue-phemex", "--lib", test, "--", "--exact",
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def passing(test):
    code, output = run(test)
    assert code == 0 and "test result: ok. 1 passed" in output, output


for module, witness, before, after in CASES:
    test = f"{module}::accepted_outbound_contract::{witness}"
    passing(test)
    path = ROOT / "safe-cex-venue-phemex/src" / f"{module}.rs"
    original = path.read_bytes()
    needle = before.encode()
    if module == "websocket_data":
        start = original.index(b"fn send<H: WsHost>")
        end = original.index(b"\n}", start) + 2
    else:
        start, end = 0, len(original)
    region = original[start:end]
    assert region.count(needle) == 1, (module, before, region.count(needle))
    try:
        path.write_bytes(original[:start] + region.replace(needle, after.encode()) + original[end:])
        code, output = run(test)
        assert code == 101 and "test result: FAILED" in output and f"---- {test} stdout ----" in output, output
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    passing(test)
    print(f"{module}: compiled named failure101; exact restoration PASS", flush=True)
