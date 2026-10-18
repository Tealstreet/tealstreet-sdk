from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/value.rs'
output = Path('/tmp/js-native-resource-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
original = b'            resource,\n            extensible: Cell::new(true),'
cases = [
    ('drop-attachment', b'            resource: resource.and(None),\n            extensible: Cell::new(true),'),
    ('leak-attachment', b'            resource: {std::mem::forget(resource.clone()); resource},\n            extensible: Cell::new(true),'),
]
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--features', 'native', '--test', 'js_native_resources',
           'native_resource_retains_exact_owner_until_the_last_alias_and_lookup_release',
           '--jobs', '1']
assert saved.count(original) == 1
for label, replacement in cases:
    try:
        source.write_bytes(saved.replace(original, replacement))
        with (output / (label + '.log')).open('wb') as log:
            result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
        receipt = (output / (label + '.log')).read_text()
        assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
        assert 'NATIVE_RESOURCE' in receipt, receipt
        print(label + ': compiled named RED', flush=True)
    finally:
        source.write_bytes(saved)
        assert source.read_bytes() == saved
    with (output / (label + '-restored.log')).open('wb') as log:
        result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (output / (label + '-restored.log')).read_text()
    print(label + ': exact restoration GREEN', flush=True)
