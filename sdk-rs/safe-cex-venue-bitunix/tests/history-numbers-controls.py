"""Serial per-row production and independent corpus-count controls."""
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[3]
crate = root / 'sdk-rs/safe-cex-venue-bitunix'
source = crate / 'src/history_numbers.rs'
test = crate / 'tests/history_numbers.rs'
original = {p: p.read_text() for p in [source, test]}
logs = root / 'sdk-rs/target/bitunix-history-number-controls'
logs.mkdir(exist_ok=True)
controls = []
controls.append(('optional-zero', {source: original[source].replace('parsed.is_finite() && parsed != 0.0', 'parsed.is_finite()')}, 'HISTORY native order optional'))
for name, assertion in [('order_to_number', 'HISTORY native order number'), ('income_to_number', 'HISTORY native income number')]:
    start = original[source].index('pub fn ' + name + '(')
    prefix, body = original[source][:start], original[source][start:]
    body = body.replace('if parsed.is_finite() { parsed }', 'if parsed.is_finite() { parsed.abs() }', 1)
    controls.append((name + '-sign', {source: prefix + body}, assertion))
# Keep the actual-source artifact unchanged. Remove primary source guards in a
# temporary witness copy; the fixed native comparison expectations must suffice.
bypass = original[test].replace('let rows: Vec<Value>', 'let mut rows: Vec<Value>')
bypass, n = re.subn(r'    assert_eq!\(rows.len\(\), 51, "HISTORY source total"\);', '', bypass)
assert n == 1
bypass, n = re.subn(r'        assert_eq!\(\n            row\["results"\].*?"HISTORY per-case helper count"\n        \);', '', bypass, flags=re.S)
assert n == 1
for mode, operation in [('empty', 'rows.clear();'), ('subset', 'rows.remove(0);'), ('extra', 'rows.push(rows[0].clone());'), ('refusals-only', 'rows.retain(|row| row["raw"].as_str().is_some_and(|raw| raw.contains("\\\\ud800")));')]:
    changed = bypass.replace('    let mut compared = [0; 3];', '    ' + operation + '\n    let mut compared = [0; 3];')
    controls.append(('corpus-' + mode, {test: changed}, 'HISTORY independent native comparison counts'))
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitunix', '--locked', '--jobs', '1', '--test', 'history_numbers', '--', '--nocapture']
for name, changes, assertion in controls:
    try:
        for path, changed in changes.items():
            assert changed != original[path], name
            path.write_text(changed)
        result = subprocess.run(command, capture_output=True, text=True)
        (logs/(name+'-fail.log')).write_text(result.stdout+result.stderr)
        assert result.returncode == 101 and 'Finished `test`' in result.stderr and assertion in result.stdout+result.stderr, (name, result.returncode, result.stdout+result.stderr)
        print('FAIL compiled 101: '+name+' / '+assertion, flush=True)
    finally:
        for path in changes:
            path.write_text(original[path])
    result = subprocess.run(command, capture_output=True, text=True)
    (logs/(name+'-pass.log')).write_text(result.stdout+result.stderr)
    assert result.returncode == 0, (name, result.returncode, result.stdout+result.stderr)
    print('PASS restored: '+name, flush=True)
print('controls proved:', len(controls))
