"""Run exclusively: compiled original-parser faults and exact source restoration."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
globals_file = root / 'safe-cex-exchange/src/js_runtime/globals.rs'
client_file = root / 'safe-cex-exchange/src/js_runtime/client.rs'
originals = {path: path.read_bytes() for path in (globals_file, client_file)}
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-exchange', '--features', 'native', '--test', 'js_json_parse', '--jobs', '1']

def run(label, failures=()):
    subprocess.run(['df', '-h', '/'], check=True)
    if shutil.disk_usage('/').free < 15 * 1024**3:
        subprocess.run(['cargo', 'clean', '--target-dir', str(root / 'target')], cwd=root, check=True)
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path('/tmp/json-parse-control-' + label + '.log').write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and 'test result: FAILED' in result.stdout, result.stdout
        assert all('test ' + name + ' ... FAILED' in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and 'test result: ok' in result.stdout, result.stdout
    print(label, 'named failure' if failures else 'passed', flush=True)

text = originals[globals_file].decode()
needle = '''safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                        &arg(&a, 0).to_js_string()?,
                    )
                    .map(super::value::from_retained_input)'''
assert text.count(needle) == 1
old = '''serde_json::from_str::<serde_json::Value>(&String::from_utf16_lossy(&arg(&a, 0).to_js_string()?))
                        .map(|v| super::value::from_json(&v))
'''
controls = [('builtin-json', globals_file, text.replace(needle, old), ['json_builtin_retains_lone_surrogate_units_and_matches_native_parse', 'json_builtin_preserves_every_escaped_utf16_code_unit'])]
controls.append(('raw-utf16-loss', globals_file, text.replace(needle, 'safe_cex_types::retained_json::RetainedJsonInput::parse_json(&String::from_utf16_lossy(&arg(&a, 0).to_js_string()?).encode_utf16().collect::<Vec<_>>()).map(super::value::from_retained_input)'), ['json_builtin_retains_lone_surrogate_units_and_matches_native_parse']))
text = originals[client_file].decode()
needle = '''safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                &text.encode_utf16().collect::<Vec<_>>(),
            )
            .map(super::value::from_retained_input)
            .unwrap_or_else(|_| Js::str(&text))'''
assert text.count(needle) == 1
controls.append(('http-json', client_file, text.replace(needle, 'serde_json::from_str(&text).map_or_else(|_| Js::str(&text), |v| from_json(&v))'), ['real_http_json_response_retains_lone_surrogate_fields_and_rows']))
run('baseline')
for label, path, mutation, failures in controls:
    try:
        path.write_text(mutation)
        run(label, failures)
    finally:
        for original, data in originals.items():
            original.write_bytes(data)
    run(label + '-restored')
