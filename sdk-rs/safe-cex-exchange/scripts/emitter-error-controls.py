from pathlib import Path
import os
import subprocess


workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-registry/src/native_account.rs"
original = source.read_bytes()
needle = b"NativeValue::from_js(code)"
replacement = b"""NativeValue::from_js(&safe_cex_exchange::js_runtime::value::from_json(
    &safe_cex_exchange::js_runtime::value::to_json(code).expect("injected JSON projection")
))"""
assert original.count(needle) == 1, "exact production error-code capture required"
mutated = original.replace(needle, replacement)


def test(name):
    result = subprocess.run(
        ["cargo", "+1.99.0", "test", "--locked", "--jobs=1", "-p", "safe-cex-registry",
         "--lib", name],
        cwd=workspace,
        env={**os.environ, "CARGO_INCREMENTAL": "0"},
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=900,
    )
    print(result.stdout, flush=True)
    return result


baseline = test("emitter_error_tests")
assert baseline.returncode == 0, "native error/future witnesses must pass before mutation"
assert "2 passed" in baseline.stdout, "both native witnesses must execute"
try:
    source.write_bytes(mutated)
    control = test("account_error_envelope_retains_native_code_presence_and_bits")
    assert control.returncode == 101, "JSON projection must fail the native code regression"
    assert "account_error_envelope_retains_native_code_presence_and_bits ... FAILED" in control.stdout
    assert "assertion failed: matches!(wire.get(\"code\"), Some(NativeValue::Undefined))" in control.stdout
    assert "could not compile" not in control.stdout, "the control must compile and reach its assertion"
finally:
    assert source.read_bytes() == mutated, "refusing to overwrite concurrent source edits"
    source.write_bytes(original)
restored = test("emitter_error_tests")
assert restored.returncode == 0 and "2 passed" in restored.stdout, "exact restoration must pass"
assert source.read_bytes() == original, "production source must be restored exactly"
print("Native error/future witnesses pass; compiled JSON-projection control fails; exact restoration passes.")
