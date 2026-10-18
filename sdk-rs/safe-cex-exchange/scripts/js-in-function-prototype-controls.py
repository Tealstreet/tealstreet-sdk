from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/library.rs'
output = Path('/tmp/js-in-function-prototype-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
original = b'    Ok(obj.has_property(&super::value::utf16(key)))'
mutated = b'    Ok(!matches!(obj, Js::Function(_)) && obj.has_property(&super::value::utf16(key)))'
assert saved.count(original) == 1
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--features', 'native', '--test', 'js_in_function_prototypes', '--jobs', '1']
try:
    source.write_bytes(saved.replace(original, mutated))
    with (output / 'function-bypass.log').open('wb') as log:
        result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
    receipt = (output / 'function-bypass.log').read_text()
    assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
    assert 'IN_FUNCTION inherited own-undefined' in receipt, receipt
    print('function-bypass: compiled named RED', flush=True)
finally:
    source.write_bytes(saved)
    assert source.read_bytes() == saved
with (output / 'restored.log').open('wb') as log:
    result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
assert result.returncode == 0, (output / 'restored.log').read_text()
print('function-bypass: exact restoration GREEN', flush=True)
