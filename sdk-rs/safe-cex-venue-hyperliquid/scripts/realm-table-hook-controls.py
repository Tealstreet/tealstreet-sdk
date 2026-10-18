from pathlib import Path
import subprocess

package = Path(__file__).resolve().parents[1]
workspace = package.parent
source = package / 'src/realm/core.rs'
original = source.read_bytes()
text = original.decode()
label = 'legacy_table_hook_failures_stop_each_consumer_and_keep_original_throw'
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-hyperliquid', '--lib', label, '--jobs', '1']

get_marker = 'view.lookup(self, key)?'
assert text.count(get_marker) == 1
start = text.index('pub(crate) fn own_keys(')
keys_marker = 'Some(view) => view.keys(self)?,'
assert text[start:].count(keys_marker) == 1
faults = [
    ('swallowed lookup', text.replace(get_marker, 'view.lookup(self, key).unwrap_or(None)', 1)),
    ('swallowed keys', text[:start] + text[start:].replace(keys_marker, 'Some(view) => view.keys(self).unwrap_or_default(),', 1)),
]
try:
    for name, fault in faults:
        source.write_text(fault)
        result = subprocess.run(command, cwd=workspace, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101, (name, output)
        assert 'error[E' not in output, output
        assert label in output and 'HL_TABLE_HOOK' in output, output
        print(name + ': compiled named RED')
        source.write_bytes(original)
        assert source.read_bytes() == original
finally:
    source.write_bytes(original)
result = subprocess.run(command, cwd=workspace, capture_output=True, text=True)
assert result.returncode == 0, result.stdout + result.stderr
assert source.read_bytes() == original
print('exact restoration: native/Node consumer witness GREEN')
