"""Production traversal mutations and a vacuous-corpus guard control, serially."""
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[3]
crate = root / 'sdk-rs/safe-cex-venue-bitunix'
source = crate/'src/response_helpers.rs'
test = crate/'tests/response_helpers.rs'
original = {p: p.read_text() for p in [source, test]}
logs = root/'sdk-rs/target/bitunix-response-helper-controls'
logs.mkdir(exist_ok=True)
controls = [
    ('record-array', {source: original[source].replace('value.is_some_and(Value::is_object)', 'value.is_some_and(|v| v.is_object() || v.is_array())')}, 'RESPONSE native isRecord'),
    ('depth-three', {source: original[source].replace('depth > 4', 'depth > 3')}, 'RESPONSE independent native length'),
    ('edge-order', {source: original[source].replace('"data",\n            "result",', '"result",\n            "data",')}, 'RESPONSE native candidate'),
]
bypass = original[test]
for pattern in [r'    assert_eq!\(rows.len\(\), 64, "RESPONSE source total"\);', r'        assert_eq!\(\n            row\["outputs"\].*?"RESPONSE source per-case count \{index\}"\n        \);', r'        assert_eq!\(\n            row\["paths"\].*?"RESPONSE source identity count \{index\}"\n        \);']:
    bypass, count = re.subn(pattern, '', bypass, flags=re.S)
    assert count == 1, pattern
for name, operation in [('empty', 'candidates.clear();'), ('subset', 'candidates.truncate(1);'), ('extra', 'if let Some(first) = candidates.first().copied() { candidates.push(first); }')]:
    changed = original[source].replace('    candidates\n}', '    '+operation+'\n    candidates\n}')
    controls.append(('native-'+name+'-source-guards-bypassed', {source: changed, test: bypass}, 'RESPONSE independent native length'))
empty = bypass.replace('let rows: Vec<Value>', 'let mut rows: Vec<Value>').replace('    let mut expected_counts', '    rows.clear();\n    let mut expected_counts')
controls.append(('source-empty-counts-bypassed', {test: empty}, 'RESPONSE native case comparisons'))
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitunix', '--locked', '--jobs', '1', '--test', 'response_helpers', '--', '--nocapture']
for name, changes, assertion in controls:
    try:
        for path, changed in changes.items():
            assert changed != original[path], name
            path.write_text(changed)
        result = subprocess.run(command, capture_output=True, text=True)
        (logs/(name+'-fail.log')).write_text(result.stdout+result.stderr)
        assert result.returncode == 101 and 'Finished `test`' in result.stderr and assertion in result.stdout+result.stderr, (name,result.returncode,result.stdout+result.stderr)
        print('FAIL compiled 101: '+name+' / '+assertion, flush=True)
    finally:
        for path in changes: path.write_text(original[path])
    result = subprocess.run(command, capture_output=True, text=True)
    (logs/(name+'-pass.log')).write_text(result.stdout+result.stderr)
    assert result.returncode == 0, (name,result.returncode,result.stdout+result.stderr)
    print('PASS restored: '+name, flush=True)
print('controls proved:', len(controls))
