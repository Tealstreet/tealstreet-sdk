"""Run exclusively: compiled carrier/writer faults, followed by exact restoration."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
typed = root / 'safe-cex-exchange/src/js_runtime/value/typed.rs'
writer = root / 'safe-cex-num/src/json_stringify.rs'
originals = {p: p.read_bytes() for p in (typed, writer)}
core = ['cargo', 'test', '--locked', '-p', 'safe-cex-exchange', '--features', 'native', '--test', 'js_runtime', 'retained_adapter_config_preserves_native_values_at_terminal_and_effective_config', '--jobs', '1']
num = ['cargo', 'test', '--locked', '-p', 'safe-cex-num', '--test', 'retained_json_writer', '--jobs', '1']

def run(label, command, failures=()):
    subprocess.run(['df', '-h', '/'], check=True)
    if shutil.disk_usage('/').free < 15 * 1024**3:
        subprocess.run(['cargo', 'clean', '--target-dir', str(root / 'target')], cwd=root, check=True)
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path('/tmp/retained-json-control-' + label + '.log').write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and 'test result: FAILED' in result.stdout, result.stdout
        assert all('test ' + name + ' ... FAILED' in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and 'test result: ok' in result.stdout, result.stdout
    print(label, 'named failure' if failures else 'passed', flush=True)

controls = []

def control(label, path, needle, replacement, command, failures):
    text = originals[path].decode()
    assert text.count(needle) == 1, (label, text.count(needle))
    controls.append((label, path, text.replace(needle, replacement), command, failures))

name = ['retained_adapter_config_preserves_native_values_at_terminal_and_effective_config']
control('old-string-refusal', typed, 'Js::String(value) => Input::Text(value.to_vec()),',
        'Js::String(value) => Input::Text(String::from_utf16(value).map_err(|_| Thrown::type_error("lone UTF-16 surrogate at a typed boundary"))?.encode_utf16().collect()),', core, name)
control('old-key-replacement', typed, 'key.to_vec(),', 'String::from_utf16_lossy(&key).encode_utf16().collect(),', core, name)
control('writer-text-replacement', writer, 'json_quote_utf16(value, &mut text);',
        'json_quote_utf16(&String::from_utf16_lossy(value).encode_utf16().collect::<Vec<_>>(), &mut text);', num,
        ['retained_writer_matches_node_for_every_utf16_unit', 'retained_writer_keeps_native_data_until_wire_projection'])
control('writer-array-subset', writer, 'for (index, value) in values.iter().enumerate() {',
        'for (index, value) in values.iter().take(1).enumerate() {', num,
        ['retained_writer_keeps_native_data_until_wire_projection'])
run('core-baseline', core)
run('num-baseline', num)
for label, path, mutation, command, failures in controls:
    try:
        path.write_text(mutation)
        run(label, command, failures)
    finally:
        for original, data in originals.items():
            original.write_bytes(data)
    run(label + '-restored', command)
