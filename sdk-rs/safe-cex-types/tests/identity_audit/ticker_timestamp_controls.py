from pathlib import Path
import os
import subprocess

target = Path('sdk-rs/safe-cex-types/src/lib.rs')
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
           '-p', 'safe-cex-num', '--test', 'ticker_timestamp', '--jobs', '1']

def control(label, mutate, assertion):
    original = target.read_text()
    try:
        target.write_text(mutate(original))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        Path('/tmp/p4-ticker-' + label + '.log').write_text(output)
        assert result.returncode == 101 and assertion in output and 'could not compile' not in output, output
        print(label, 'compiled 101:', assertion, flush=True)
    finally:
        target.write_text(original)

def wire_part(source, mutate):
    start = source.index('    struct WireTicker {')
    end = source.index('\n    }', start)
    return source[:start] + mutate(source[start:end]) + source[end:]

def swap_names(source):
    def change(wire):
        wire = wire.replace('        quote_volume: f64,', '        #[serde(rename = "timestamp")]\n        quote_volume: f64,')
        return wire.replace('        timestamp: Option<f64>,', '        #[serde(rename = "quoteVolume")]\n        timestamp: Option<f64>,')
    return wire_part(source, change)

def erase_zero(source):
    helper = '''fn erase_ticker_zero<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    deserialize_present(d).map(|value: Option<f64>| value.map(|v| if v == 0.0 { 0.0 } else { v }))
}
'''
    def change(wire):
        timestamp = wire.index('        timestamp: Option<f64>,')
        attribute = wire.rfind('deserialize_with = "deserialize_present"', 0, timestamp)
        assert attribute != -1
        return wire[:attribute] + wire[attribute:].replace('deserialize_with = "deserialize_present"', 'deserialize_with = "crate::erase_ticker_zero"', 1)
    return wire_part(source, change).replace('\nuse serde::', '\n' + helper + 'use serde::', 1)

control('field-swap', swap_names, 'TICKER-TIMESTAMP native clock identity')
control('signed-zero', erase_zero, 'TICKER-TIMESTAMP native clock identity')
result = subprocess.run(command, capture_output=True, text=True)
Path('/tmp/p4-ticker-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('RESTORED 1/1', flush=True)

store_command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
                 '-p', 'safe-cex-store', '--test', 'ticker_timestamp', '--jobs', '1']
for label, filename, old, new in [
    ('patch-drop', 'sdk-rs/safe-cex-store/src/model_patch.rs',
     '            "timestamp" => self.timestamp = Some(number(field, value)?),',
     '            "timestamp" => {}'),
    ('alias-drop', 'sdk-rs/safe-cex-store/src/store.rs',
     '    target.timestamp = source.timestamp;', ''),
]:
    path = Path(filename)
    original = path.read_text()
    assert original.count(old) == 1
    try:
        path.write_text(original.replace(old, new))
        result = subprocess.run(store_command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        Path('/tmp/p4-ticker-' + label + '.log').write_text(output)
        assertion = 'TICKER-TIMESTAMP Store numeric patch identity'
        assert result.returncode == 101 and assertion in output and 'could not compile' not in output, output
        print(label, 'compiled 101:', assertion, flush=True)
    finally:
        path.write_text(original)
result = subprocess.run(store_command, capture_output=True, text=True)
Path('/tmp/p4-ticker-store-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('STORE RESTORED 1/1', flush=True)
