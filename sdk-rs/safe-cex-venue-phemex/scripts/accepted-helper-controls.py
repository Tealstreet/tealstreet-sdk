"""Compiling fail/restore controls for nine individually reviewed helper owners."""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ROOT / "safe-cex-venue-phemex/src"


def run(test):
    result = subprocess.run([
        "cargo", "test", "--locked", "--jobs", "1", "-q", "-p",
        "safe-cex-venue-phemex", "--lib", test, "--", "--exact",
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def passing(test):
    code, output = run(test)
    assert code == 0 and "test result: ok. 1 passed" in output, output


cases = [
    ("private_session", "units", "utf16_encoder_preserves_native_units"),
    ("public_routing", "units", "utf16_encoder_preserves_native_units"),
    ("rest_data", "units", "utf16_encoder_preserves_native_units"),
    ("start_order_formatting", "u", "utf16_encoder_preserves_native_units"),
    ("websocket_data", "u", "utf16_encoder_preserves_native_units"),
    ("rest_account_control", "endpoint", "endpoint_lookup_preserves_selected_wire_paths"),
    ("rest_order_control", "endpoint", "endpoint_lookup_preserves_selected_wire_paths"),
    ("rest_data", "endpoint", "endpoint_lookup_preserves_selected_wire_paths"),
    ("ticker_parser", "type_error", "type_error_constructor_preserves_name_and_message"),
]
for module, function, witness in cases:
    test = f"{module}::accepted_native_contract::{witness}"
    passing(test)
    path = SOURCES / f"{module}.rs"
    original = path.read_bytes()
    text = original.decode()
    match = re.search(rf"(?m)^fn {function}\([^\n]*\{{\n.*?^\}}", text, re.S)
    assert match, (module, function)
    before = match.group()
    if function in ("units", "u"):
        assert before.count(".encode_utf16()") == 1
        after = before.replace(".encode_utf16()", ".bytes().map(u16::from)")
    elif function == "endpoint":
        after = before[:before.index("{\n")] + '{\n    let _ = key;\n    "/wrong-path"\n}'
    else:
        assert before.count('"TypeError"') == 1
        after = before.replace('"TypeError"', '"Error"')
    try:
        path.write_text(text[:match.start()] + after + text[match.end():])
        code, output = run(test)
        assert code == 101 and "test result: FAILED" in output and f"---- {test} stdout ----" in output, output
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    passing(test)
    print(f"{module}::{function}: compiled named failure101; exact restoration PASS", flush=True)
