from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/value.rs'
output = Path('/tmp/js-write-observer-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
notify = b'            observer(self, key, &value)?;'
write = b'        self.set_property(key, value.clone())?;'
cases = [
    ('missing-publication', saved.replace(notify, b'            let _ = observer;'),
     'every_successful_options_write_is_immediately_visible_to_native_account_readers'),
    ('premature-publication', saved.replace(write, b'').replace(notify, notify + b'\n        }\n' + write + b'\n        if false {'),
     'every_successful_options_write_is_immediately_visible_to_native_account_readers'),
]
assert saved.count(notify) == saved.count(write) == 1
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--features', 'native', '--test', 'js_write_observers', '--jobs', '1']
for label, mutated, test in cases:
    try:
        source.write_bytes(mutated)
        with (output / (label + '.log')).open('wb') as log:
            result = subprocess.run(command + [test], cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
        receipt = (output / (label + '.log')).read_text()
        assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
        assert 'WRITE_OBSERVER' in receipt, receipt
        print(label + ': compiled named RED', flush=True)
    finally:
        source.write_bytes(saved)
        assert source.read_bytes() == saved
    with (output / (label + '-restored.log')).open('wb') as log:
        result = subprocess.run(command + [test], cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (output / (label + '-restored.log')).read_text()
    print(label + ': exact restoration GREEN', flush=True)
