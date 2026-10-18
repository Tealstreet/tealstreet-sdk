from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/for_in.rs'
output = Path('/tmp/js-for-in-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
mutations = [('hidden-shadow', b'if enumerable {', b'if enumerable || current.is_object() {'),
             ('missing-prototype', b'self.current = current.prototype();', b'self.current = None;')]
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--test', 'js_for_in', '--jobs', '1']
for label, old, new in mutations:
    assert saved.count(old) == 1
    try:
        source.write_bytes(saved.replace(old, new))
        with (output / (label + '.log')).open('wb') as log:
            result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
        receipt = (output / (label + '.log')).read_text()
        assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
        assert 'FOR_IN' in receipt, receipt
        print(label + ': compiled named RED', flush=True)
    finally:
        source.write_bytes(saved)
        assert source.read_bytes() == saved
    with (output / (label + '-restored.log')).open('wb') as log:
        result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (output / (label + '-restored.log')).read_text()
    print(label + ': exact restoration GREEN', flush=True)
