#!/usr/bin/env python3
"""Compile each wrong native response-shape/conversion output, then restore exactly."""
from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-binance'
path = crate / 'src/response_shape.rs'
runner = crate / 'scripts/response-shape-contracts.mjs'
module = 'safe_cex_venue_binance::response_shape::'
original = path.read_bytes()
cases = [
    ('describe_binance_response_shape', 'fn describe_binance_response_shape(', b'\n    let _ = payload;\n    String::new()\n'),
    ('describe_binance_pm_response_shape', 'fn describe_binance_pm_response_shape(', b'\n    let _ = payload;\n    String::new()\n'),
    ('describe', 'fn describe(', b'\n    let _ = payload;\n    String::new()\n'),
    ('require_binance_array_response', "fn require_binance_array_response<'a>(", b'\n    let _ = (endpoint,payload);\n    Err(BinanceResponseError{name:"Error",message:String::new()})\n'),
    ('require_binance_pm_array_response', "fn require_binance_pm_array_response<'a>(", b'\n    let _ = (endpoint,payload);\n    Err(BinanceResponseError{name:"Error",message:String::new()})\n'),
    ('native_malformed', 'fn native_malformed(', b'\n    let _ = (venue,endpoint,shape,message);\n    Thrown::new(error_object("Error",""))\n'),
    ('BinanceResponseError', 'fn from(error: JsCoercionError)', b'\n    Self{name:"Error",message:error.message.into()}\n'),
]


def run(owner=None):
    args = ['node', str(runner)]
    if owner:
        args.append('--expect-output-failure=' + owner)
    result = subprocess.run(args, cwd=workspace, capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


native = {
    'describe_binance_response_shape': ('--test', 'accepted_response_shapes', 'futures_response_descriptor_has_source_key_order_and_cutoff'),
    'describe_binance_pm_response_shape': ('--test', 'accepted_response_shapes', 'pm_response_descriptor_has_source_key_order_and_cutoff'),
    'describe': ('--lib', 'response_shape::accepted_contract_tests::private_descriptor_preserves_source_shape'),
    'native_malformed': ('--lib', 'response_shape::accepted_contract_tests::private_malformed_constructor_preserves_error_fields'),
}


def native_run(ident):
    cmd = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-binance', '--jobs', '1', *native[ident], '--', '--exact', '--nocapture']
    result = subprocess.run(cmd, cwd=workspace, env={**os.environ, 'RUSTC_BOOTSTRAP': '1'}, capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


# Keep native-test controls together before the production metadata/runtime controls.
cases = [cases[index] for index in [0, 1, 2, 5, 3, 4, 6]]
production_baselined = False

try:
    for ident, marker, replacement in cases:
        marker = marker.encode()
        assert original.count(marker) == 1, ident
        start = original.index(b'{', original.index(marker))
        end, depth = start + 1, 1
        # Reviewed source literals contain only balanced format-string braces.
        while depth:
            depth += (original[end] == ord('{')) - (original[end] == ord('}'))
            end += 1
        owner = module + ident
        if ident not in native and not production_baselined:
            baseline, output = run()
            assert (baseline.returncode == 0 and 'Three production response-shape output probes and four explicit diagnostic classifications passed' in output
                    and '0 failed owners' in output), output[-8000:]
            print('BASELINE PASS: production3 and diagnostic classifications4', flush=True)
            production_baselined = True
        path.write_bytes(original[:start + 1] + replacement + original[end - 1:])
        if ident in native:
            changed, output = native_run(ident)
            test = native[ident][-1]
            assert (changed.returncode != 0 and f'test {test} ... FAILED' in output
                    and 'test result: FAILED' in output and 'could not compile' not in output), output[-8000:]
            print(f'CONTROL PASS: {owner}; its named native diagnostic test fails after compilation', flush=True)
            path.write_bytes(original)
            restored, output = native_run(ident)
            assert (restored.returncode == 0 and f'test {test} ... ok' in output
                    and '1 passed; 0 failed; 0 ignored' in output), output[-8000:]
            print(f'RESTORED PASS: {owner}; its direct named native test passes', flush=True)
        else:
            changed, output = run(owner)
            assert (changed.returncode == 0 and f'NATIVE_OUTPUT_FAILURE: {owner}' in output
                    and 'compile failure is not a control' not in output), output[-8000:]
            print(f'CONTROL PASS: {owner}; its own actual production output fails', flush=True)
            path.write_bytes(original)
            restored, output = run()
            assert (restored.returncode == 0 and 'Three production response-shape output probes and four explicit diagnostic classifications passed' in output
                    and '0 failed owners' in output), output[-8000:]
            print(f'RESTORED PASS: {owner}; production3 and diagnostic classifications4 pass', flush=True)
finally:
    path.write_bytes(original)
