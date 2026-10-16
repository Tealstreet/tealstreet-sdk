from pathlib import Path
import os
import subprocess

path = Path('sdk-rs/safe-cex-types/src/retained_json.rs')
original = path.read_text()
command = [os.environ.get('CARGO', 'cargo'), 'test', '--locked', '--manifest-path',
           'sdk-rs/Cargo.toml', '-p', 'safe-cex-types', '--lib',
           'retained_array_cardinality_is_independent_of_decoder_output', '--jobs', '1']
old = '''        while let Some(value) = seq.next_element()? {
            entries.push(value);
        }'''
wrong = '''        while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
        entries.clear();'''
assert original.count(old) == 1
mutant = original.replace(old, wrong)
guards = [
    '        assert_eq!(entries.len(), 3, "RETAINED-CARDINALITY captured length");\n',
    '        assert_eq!(array.len(), 3, "RETAINED-CARDINALITY projected length");\n',
]
for guard in guards:
    assert mutant.count(guard) == 1
bypassed = mutant
for guard in guards:
    bypassed = bypassed.replace(guard, '')
bypassed = bypassed.replace('RetainedJsonInput::Array(entries) = &input',
                            'RetainedJsonInput::Array(_entries) = &input')
try:
    for label, source, needle in [
        ('consuming-empty', mutant, 'RETAINED-CARDINALITY captured length'),
        ('consuming-empty-length-bypass', bypassed, 'RETAINED-CARDINALITY executed comparisons'),
    ]:
        path.write_text(source)
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        Path('/tmp/p4-retained-' + label + '.log').write_text(output)
        assert result.returncode == 101 and needle in output and 'could not compile' not in output, output
        print(label, 'COMPILED; failed:', needle, flush=True)
finally:
    path.write_text(original)
result = subprocess.run(command, capture_output=True, text=True)
Path('/tmp/p4-retained-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('RESTORED PASS', flush=True)
