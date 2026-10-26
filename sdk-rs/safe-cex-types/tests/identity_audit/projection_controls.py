from pathlib import Path
import argparse
import os
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--subset-only', action='store_true')
arguments = parser.parse_args()

path = Path('sdk-rs/safe-cex-types/src/retained_json.rs')
original = path.read_text()
command = [os.environ.get('CARGO', 'cargo'), 'test', '--locked', '--manifest-path',
           'sdk-rs/Cargo.toml', '-p', 'safe-cex-types', '--lib', '--jobs', '1']
arms = {
    'array': '''        RetainedJsonInput::Array(values) => Value::Array(
            values
                .into_iter()
                .map(into_value)
                .collect::<Result<_, _>>()?,
        ),''',
    'object': '''        RetainedJsonInput::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, into_value(value)?)))
                .collect::<Result<_, serde_json::Error>>()?,
        ),''',
}

def assertion(label):
    assert original.count('"' + label + '"') == 1
    position = original.index('"' + label + '"')
    start = original.rfind('        assert', 0, position)
    end = original.index(';\n', position) + 2
    assert start >= 0
    return original[start:end]

try:
    for arm, old in arms.items():
        assert original.count(old) == 1
        container = 'Array' if arm == 'array' else 'Object'
        empty = f'''        RetainedJsonInput::{container}(values) => {{
            for {'value' if arm == 'array' else '(_, value)'} in values {{ into_value(value)?; }}
            Value::{container}(Default::default())
        }},'''
        extra = 'RetainedJsonInput::Lexeme("99".into())'
        if arm == 'object':
            extra = '("extra".into(), ' + extra + ')'
        collect = '.collect::<Result<_, _>>()?' if arm == 'array' else '.collect::<Result<_, serde_json::Error>>()?'
        projected = '.collect::<Result<Vec<_>, serde_json::Error>>()?'
        subset = old.replace(collect, projected + '.into_iter().enumerate().filter(|(index, _)| *index == 0).map(|(_, value)| value).collect()')
        assert subset != old
        replacements = {
            'consuming-empty': empty,
            'consuming-subset': subset,
            'extra-native': old.replace('.into_iter()',
                                        '.into_iter().chain(std::iter::once(' + extra + '))'),
        }
        if arguments.subset_only:
            replacements = {'consuming-subset': subset}
        guard = assertion(f'RETAINED-PROJECTION {arm} native length')
        source_guard = assertion(f'RETAINED-PROJECTION {arm} source length')
        assert original.count(guard) == 1
        assert original.count(source_guard) == 1
        for label, replacement in replacements.items():
            for bypass in [False, True]:
                source = original.replace(old, replacement)
                if bypass:
                    source = source.replace(guard, '')
                    source = source.replace(source_guard, '')
                needle = f'RETAINED-PROJECTION {arm} native length'
                if bypass:
                    needle = f'RETAINED-PROJECTION {arm} ' + (
                        ('native index coverage' if arm == 'array' else 'native key coverage')
                        if label == 'extra-native' else 'comparisons')
                path.write_text(source)
                result = subprocess.run(command + ['retained_projection_' + arm],
                                        capture_output=True, text=True)
                output = result.stdout + result.stderr
                name = arm + '-' + label + ('-guard-bypass' if bypass else '')
                Path('/tmp/p4-projection-' + name + '.log').write_text(output)
                assert result.returncode == 101 and needle in output and 'could not compile' not in output, output
                print(name, 'COMPILED; failed:', needle, flush=True)
finally:
    path.write_text(original)
result = subprocess.run(command + ['retained_projection_'], capture_output=True, text=True)
Path('/tmp/p4-projection-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('RESTORED 2/2', flush=True)
