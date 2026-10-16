#!/usr/bin/env python3
"""Compile each helper defect; native witnesses or the direct decoder must fail."""
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-kraken/src/helpers.rs'
runner = workspace / 'safe-cex-venue-kraken/scripts/helper-contracts.mjs'
original = source.read_bytes()
owner = 'safe_cex_venue_kraken::helpers::parse_trigger_signal'
native = [
    ('sort', b'collator.compare(left, right)', b'collator.compare(right, left)',
     'sort_compares_literal_operands_without_decoding_them'),
    ('create_kraken_api_error',
     b'error.metadata.response = response.cloned();\n    error.metadata.original_error = cause.cloned();',
     b'error.metadata.response = cause.cloned();\n    error.metadata.original_error = response.cloned();',
     'error_constructor_preserves_literal_message_distinct_snapshots_and_presence'),
    ('create_kraken_spot_client_order_id', b'usize::from(*byte & 15)', b'usize::from(*byte & 7)',
     'id_factory_executes_host_entropy_and_retains_pool_after_failure'),
]


def cargo(test=None):
    args = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-kraken', '--jobs', '1', '--test', 'accepted_helpers']
    args += [test, '--', '--exact', '--nocapture'] if test else ['--test', 'helpers', '--', '--include-ignored']
    result = subprocess.run(args, cwd=workspace, env={**os.environ, 'RUSTC_BOOTSTRAP': '1'},
                            capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


def production(control=False):
    args = ['node', str(runner)]
    if control:
        args.append('--expect-output-failure=' + owner)
    result = subprocess.run(args, cwd=workspace, capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


baseline, output = cargo()
assert baseline.returncode == 0 and '4 passed; 0 failed; 0 ignored' in output and '5 passed; 0 failed; 0 ignored' in output, output[-8000:]
print('BASELINE PASS: native4 + actual-source helpers5, zero ignored', flush=True)
try:
    for name, old, new, test in native:
        assert original.count(old) == 1, name
        source.write_bytes(original.replace(old, new))
        changed, output = cargo(test)
        assert (changed.returncode not in (0, 126) and f'test {test} ... FAILED' in output
                and 'test result: FAILED' in output and 'could not compile' not in output), output[-8000:]
        print(f'CONTROL PASS: {name}; its named native witness fails after compilation', flush=True)
        source.write_bytes(original)
        restored, output = cargo(test)
        assert restored.returncode == 0 and f'test {test} ... ok' in output and '1 passed; 0 failed; 0 ignored' in output, output[-8000:]
        print(f'RESTORED PASS: {name}; its actual named native witness passes', flush=True)

    baseline, output = production()
    assert baseline.returncode == 0 and 'One production helper output probe and three explicit non-decoder classifications passed' in output and '0 failed owners' in output, output[-8000:]
    print('BASELINE PASS: production decoder1 and role classifications3', flush=True)
    old = b'"spot" => Some(OrderStopTriggerType::IndexPrice)'
    new = b'"spot" => Some(OrderStopTriggerType::MarkPrice)'
    assert original.count(old) == 1
    source.write_bytes(original.replace(old, new))
    changed, output = production(control=True)
    assert changed.returncode == 0 and f'NATIVE_OUTPUT_FAILURE: {owner}' in output, output[-8000:]
    print('CONTROL PASS: parse_trigger_signal; its own compiled production output mismatches', flush=True)
finally:
    source.write_bytes(original)

restored, output = production()
assert restored.returncode == 0 and '0 failed owners' in output, output[-8000:]
print('RESTORED PASS: production decoder1 and role classifications3', flush=True)
restored, output = cargo()
assert restored.returncode == 0 and '4 passed; 0 failed; 0 ignored' in output and '5 passed; 0 failed; 0 ignored' in output, output[-8000:]
assert source.read_bytes() == original
print('FINAL PASS: exact source restored; native4 + actual-source helpers5, zero ignored', flush=True)
