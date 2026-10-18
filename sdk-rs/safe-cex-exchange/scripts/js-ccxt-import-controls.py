from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/ccxt.rs'
output = Path('/tmp/js-ccxt-import-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
mutations = [
    ('coercive-mode', b'v.strict_equals(&num(n))', b'v.to_number().is_ok_and(|value| value == n)'),
    ('missing-third-read', b'        o.get_property(k)?\n', b'        Js::Number(1.)\n'),
]
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--features', 'native', '--test', 'js_ccxt_imports', '--jobs', '1']
for label, old, new in mutations:
    assert saved.count(old) == 1
    try:
        source.write_bytes(saved.replace(old, new))
        with (output / (label + '.log')).open('wb') as log:
            result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
        receipt = (output / (label + '.log')).read_text()
        assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
        assert 'CCXT_IMPORT' in receipt, receipt
        print(label + ': compiled named RED', flush=True)
    finally:
        source.write_bytes(saved)
        assert source.read_bytes() == saved
    with (output / (label + '-restored.log')).open('wb') as log:
        result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (output / (label + '-restored.log')).read_text()
    print(label + ': exact restoration GREEN', flush=True)
