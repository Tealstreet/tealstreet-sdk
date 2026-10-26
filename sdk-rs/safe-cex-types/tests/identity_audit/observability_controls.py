from pathlib import Path
import os
import subprocess

cargo = [os.environ.get('CARGO', 'cargo'), 'test', '--locked', '--manifest-path',
         'sdk-rs/Cargo.toml', '-p', 'safe-cex-types', '--test', 'import_observability', '--jobs', '1']

def control(label, filename, mutate, command, needle):
    path = Path(filename)
    original = path.read_text()
    wrong = mutate(original)
    assert wrong != original
    try:
        path.write_text(wrong)
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        Path('/tmp/p4-observability-' + label + '.log').write_text(output)
        assert result.returncode != 0 and needle in output and 'could not compile' not in output, output
        print(label, 'named failure:', needle, flush=True)
    finally:
        path.write_text(original)

def replace_units(source):
    old = '    let Ok(mut parsed) = serde_json::from_str::<Value>(raw) else {'
    assert source.count(old) == 1
    new = r'''    let replacement = raw.replace(r"\ud800", r"\ufffd").replace(r"\udc00", r"\ufffd");
    let Ok(mut parsed) = serde_json::from_str::<Value>(&replacement) else {'''
    return source.replace(old, new)

def refuse_ascii(source):
    old = '    let Ok(mut parsed) = serde_json::from_str::<Value>(raw) else {'
    assert source.count(old) == 1
    new = r'''    if raw.contains(r"\\ud800") { return invalid("Invalid JSON format".into()); }
''' + old
    return source.replace(old, new)

accounts = 'sdk-rs/safe-cex-types/src/accounts.rs'
control('native-replacement', accounts, replace_units, cargo,
        'IMPORT-OBSERVABILITY native acceptance unknown-high')
control('native-ascii-refusal', accounts, refuse_ascii, cargo,
        'IMPORT-OBSERVABILITY native acceptance unknown-ascii-escape')
oracle = 'sdk-rs/safe-cex-types/tests/import_observability_oracle.cjs'
control('capture-unit-erasure', oracle,
        lambda s: s.replace('value.charCodeAt(index)',
                            '(value.charCodeAt(index) === 0xd800 ? 0xfffd : value.charCodeAt(index))'),
        cargo, 'IMPORT-OBSERVABILITY returned unit identity')
for label, mutation in [('source-empty', 'rows.length = 0;'), ('source-subset', 'rows.pop();')]:
    control(label, oracle, lambda s, mutation=mutation: s.replace(
        'process.stdout.write', mutation + '\nprocess.stdout.write'), cargo,
        'IMPORT-OBSERVABILITY source total')
# The diagnostic consumer control moved with its test to tealstreet-next
# scripts/sdk-diagnostic-observability.test.cjs; its consumer is apps/web.
for label, command in [('import', cargo)]:
    result = subprocess.run(command, capture_output=True, text=True)
    Path('/tmp/p4-observability-' + label + '-restored.log').write_text(result.stdout + result.stderr)
    assert result.returncode == 0, result.stdout + result.stderr
    print(label, 'RESTORED PASS', flush=True)
