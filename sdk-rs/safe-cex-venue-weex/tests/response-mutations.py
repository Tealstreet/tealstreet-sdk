"""Serial production swaps and census/capture negative controls; one Node per corpus."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
source = crate / 'src/response_types.rs'
oracle = crate / 'tests/response-oracle.cjs'
capture = crate / 'tests/types-oracle.cjs'
originals = {p: p.read_text() for p in (source, oracle, capture)}
command = [os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test',
           '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex',
           '--jobs', '1', '--test', 'response_types', '--', '--nocapture']

def swap(text, left, right, left_wire, right_wire):
    # Rename BOTH Serialize public fields and Deserialize private remote fields.
    # A bytes-only roundtrip would still pass; native field assertions must fail.
    for prefix in ('    pub ', '    '):
        text = text.replace(prefix + left + ': Option', f'    #[serde(rename = "{right_wire}")]\n' + prefix + left + ': Option')
        text = text.replace(prefix + right + ': Option', f'    #[serde(rename = "{left_wire}")]\n' + prefix + right + ': Option')
    return text

controls = [
    (oracle, originals[oracle].replace("if (Array.isArray(value)) return { kind: 'array', values: value.map(capture) };", "if (Array.isArray(value)) return { kind: 'object', entries: value.map(capture) };"), 'capture_array_object'),
    (capture, originals[capture].replace('arch: process.arch, ', ''), 'capture_arch'),
    (source, originals[source].replace('.decode_with(NumberOrStringWire::deserialize)', '.decode_with(NumberOrStringWire::deserialize)\n            .map(|v| match v { Self::Number(n) => Self::Number(n.abs()), other => other })'), 'native::kline_signed_zero::direct_raw'),
    (source, swap(originals[source], 'assets', 'symbols', 'assets', 'symbols'), 'native::info_distinct::direct_raw'),
    (source, swap(originals[source], 'order_id', 'client_order_id', 'orderId', 'clientOrderId'), 'native::order_distinct::direct_raw'),
    (source, swap(originals[source], 'error_code', 'error_message', 'errorCode', 'errorMessage'), 'native::order_distinct::direct_raw'),
    (oracle, originals[oracle].replace('checker.getPropertiesOfType(type).map', "checker.getPropertiesOfType(type).filter(p => p.name !== 'inheritedRequired').map"), 'census_inherited_required'),
    (capture, originals[capture].replace('nodeMajor: Number(process.versions.node.split(\'.\')[0]),', ''), 'capture_node_major'),
    (capture, originals[capture].replace('buffer.writeBigUInt64BE(0x7ff8000000000000n)', 'buffer.writeBigUInt64BE(0xfff8000000000000n)'), 'capture_nan_canonical'),
]
try:
    for file, mutated, assertion in controls:
        assert mutated != originals[file], assertion
        file.write_text(mutated)
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode != 0 and assertion in output, output
        print(f'FAIL observed: {assertion}', flush=True)
        file.write_text(originals[file])
finally:
    for file, original in originals.items():
        file.write_text(original)
subprocess.run(command, check=True)
print(f'Restored PASS after {len(controls)} response/census/capture controls', flush=True)
