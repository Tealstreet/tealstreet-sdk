#!/usr/bin/env python3
"""Disconnect each batch helper, require a native binding failure, restore."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-binance/src/order_batches.rs'
original = source.read_bytes()
binding = 'private_order_aliases::native_bulk_placement_retains_post_result_until_signed_http_settles'
controls = [
    ('endpoint', binding, b'\n    let _ = (venue,key);\n    "/fapi/v1/batchOrders"\n'),
    ('post', binding, b'\n    let _ = (b,v,key,payload);\n    Ok(EventValue::Undefined)\n'),
    ('dense', binding, b'\n    let _ = (v,expr,method);\n    Ok(Vec::new())\n'),
    ('iterable', binding, b'\n    let _ = v;\n    Err(type_error(format!("{expression} is not iterable")))\n'),
    ('id', binding, b'\n    let _ = (v,key,expression);\n    Ok(EventValue::Undefined)\n'),
]
logs = workspace / 'target/order-batch-controls'
logs.mkdir(parents=True, exist_ok=True)


def run(test, label):
    result = subprocess.run([
        'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-binance',
        '--features', 'native', '--test', 'private_classes', test,
        '--', '--exact', '--nocapture',
    ], cwd=workspace, capture_output=True, text=True, timeout=900)
    output = result.stdout + result.stderr
    (logs / f'{label}.log').write_text(output)
    return result.returncode, output


try:
    for name, test, replacement in controls:
        assert source.read_bytes() == original, 'source changed outside this control'
        marker = ('fn ' + name + '(').encode()
        assert original.count(marker) == 1, name
        start = original.index(b'{', original.index(marker))
        end, depth = start + 1, 1
        while depth:
            depth += (original[end] == 123) - (original[end] == 125)
            end += 1
        source.write_bytes(original[:start + 1] + replacement + original[end - 1:])
        code, output = run(test, name + '-fault')
        assert code == 101 and f'test {test} ... FAILED' in output, output[-8000:]
        assert 'test result: FAILED' in output and 'could not compile' not in output, output[-8000:]
        print(f'CONTROL PASS {name}: compiled real-native binding failure', flush=True)
        source.write_bytes(original)
        code, output = run(test, name + '-restored')
        assert code == 0 and f'test {test} ... ok' in output, output[-8000:]
        assert '1 passed; 0 failed' in output, output[-8000:]
        print(f'RESTORED PASS {name}: exact original source and real-native binding', flush=True)
finally:
    source.write_bytes(original)
