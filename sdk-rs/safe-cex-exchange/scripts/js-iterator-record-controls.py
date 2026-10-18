from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/iterator.rs'
output = Path('/tmp/js-iterator-record-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
original = b'if !result.is_object() {'
mutated = b'if result.is_undefined() {'
assert saved.count(original) == 1
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange', '--lib',
           'js_runtime::iterator_record_tests', '--jobs', '1']
environment = dict(os.environ, CARGO_INCREMENTAL='0')
try:
    source.write_bytes(saved.replace(original, mutated))
    with (output / 'close-result.log').open('wb') as log:
        result = subprocess.run(command, cwd=workspace, env=environment,
                                stdout=log, stderr=subprocess.STDOUT)
    receipt = (output / 'close-result.log').read_text()
    assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
    assert 'JS_ITERATOR_RECORD normal close and elision' in receipt, receipt
    print('Compiled primitive-close-result mutation: named RED', flush=True)
finally:
    source.write_bytes(saved)
    assert source.read_bytes() == saved
with (output / 'restored.log').open('wb') as log:
    result = subprocess.run(command, cwd=workspace, env=environment,
                            stdout=log, stderr=subprocess.STDOUT)
assert result.returncode == 0, (output / 'restored.log').read_text()
print('Exact restoration: native/source iterator witnesses GREEN', flush=True)
