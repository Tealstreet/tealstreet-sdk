#!/usr/bin/env python3
"""Extra outputs and bypassed source guards, each against a scoped test target."""
import argparse
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
parser = argparse.ArgumentParser()
parser.add_argument('--group', choices=['types', 'constants', 'scalars', 'guard-removal'])
selected = parser.parse_args().group
completed = 0

def run(target):
    return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
        '-p', 'safe-cex-venue-bingx', '--test', target, '--jobs', '1', '--locked', '--', '--nocapture'],
        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

def prove(label, target, changes, assertion):
    global completed
    originals = {path: path.read_bytes() for path in changes}
    try:
        for path, changed in changes.items():
            assert changed != originals[path].decode(), label
            path.write_text(changed)
        result = run(target)
        assert result.returncode == 101 and assertion in result.stdout and 'test result: FAILED' in result.stdout, (label, result.stdout)
        print(f'{label}: compiled; exit101 at {assertion}', flush=True)
    finally:
        for path, original in originals.items():
            path.write_bytes(original)
    result = run(target)
    assert result.returncode == 0 and 'test result: ok.' in result.stdout, (label, result.stdout)
    completed += 1
    print(f'{label}: restored/passed', flush=True)

def bypass_types(text):
    start = text.index('    assert_eq!(cases.len(), 883,')
    end = text.index('    let mut compared_routes = 0;', start)
    text = text[:start] + '    if false {\n' + text[start:end] + '    }\n' + text[end:]
    needle = '    if case["valid"] == true\n        && matches!'
    assert text.count(needle) == 1
    return text.replace(needle, '    if false && case["valid"] == true\n        && matches!')

if selected in (None, 'types'):
    source = crate / 'src/types.rs'
    test = crate / 'tests/types.rs'
    oracle = crate / 'tests/types_oracle.cjs'
    for name in ['FetchBalanceResponse', 'FetchPositionsResponse']:
        for label, action in [('empty', 'value.data.clear();'), ('subset', 'value.data.truncate(1);'),
                              ('extra', 'if let Some(first) = value.data.first().cloned() { value.data.push(first); }')]:
            original = source.read_text()
            start = original.index("impl<'de> Deserialize<'de> for " + name + ' {')
            end = original.index('\n}', start)
            part = original[start:end].replace('.map_err(serde::de::Error::custom)',
                '.map(|mut value| { ' + action + ' value })\n            .map_err(serde::de::Error::custom)')
            changed = original[:start] + part + original[end:]
            prove(name + '/' + label + '/source-guards-bypassed', 'types',
                  {source: changed, test: bypass_types(test.read_text())}, 'BINGX-CARDINALITY native array length')
    before = 'names: Object.keys(declarations), cases, heritage: true'
    original = oracle.read_text()
    assert before in original
    prove('types/empty-corpus/source-guards-bypassed', 'types',
          {oracle: original.replace(before, 'names: Object.keys(declarations), cases: [], heritage: true'),
           test: bypass_types(test.read_text())}, 'BINGX-CARDINALITY executed route comparisons')
    marker = 'process.stdout.write(JSON.stringify('
    assert original.count(marker) == 1
    prove('types/empty-expected-object', 'types',
          {oracle: original.replace(marker, "for (const c of cases) if (c.name === 'BINGX_BALANCE' && c.native) c.native = {};\n" + marker)},
          'native_fields BINGX_BALANCE')

if selected in (None, 'constants'):
    source = crate / 'src/constants.rs'
    test = crate / 'tests/constants.rs'
    oracle = crate / 'tests/constants_oracle.cjs'
    original = source.read_text()
    pattern = r'pub const ORDER_SIDE: \[\(&str, OrderSide\); 2\] = \[(.*?)\];'
    match = re.search(pattern, original, re.S)
    assert match
    test_text = test.read_text()
    start = test_text.index('    assert_eq!(\n        actual.as_object()')
    end = test_text.index('    // serde', start)
    bypassed = test_text[:start] + '    if false {\n' + test_text[start:end] + '    }\n' + test_text[end:]
    for count in [0, 1, 3]:
        rows = re.findall(r'\("[^"]+", OrderSide::\w+\)', match[1])
        assert len(rows) == 2
        rows = (rows + ['("EXTRA", OrderSide::Buy)'])[:count]
        replacement = f'pub const ORDER_SIDE: [(&str, OrderSide); {count}] = [' + ','.join(rows) + '];'
        changed = original[:match.start()] + replacement + original[match.end():]
        prove(f'ORDER_SIDE/{count}/source-census-bypassed', 'constants', {source: changed, test: bypassed}, 'ordered constant ORDER_SIDE')
    test_text = test.read_text()
    needle = 'probes.len(),\n        15,'
    assert needle in test_text
    for expr in ['[]', 'keys.slice(1)']:
        original = oracle.read_text()
        before = 'inherited[name] = keys.map(key => {'
        assert before in original
        prove('constants/probes-' + expr + '/source-count-bypassed', 'constants',
              {oracle: original.replace(before, f'inherited[name] = {expr}.map(key => {{'),
               test: test_text.replace(needle, 'probes.len(),\n        probes.len(),')}, 'BINGX-CARDINALITY inherited comparisons')

if selected in (None, 'scalars'):
    for target, variable, expected_count, assertion in [
        ('sort', 'cases', 784, 'BINGX-CARDINALITY sort comparisons'),
        ('ticker', 'cases', 168, 'BINGX-CARDINALITY bid/ask comparisons'),
        ('errors', 'build', 70, 'BINGX-CARDINALITY build comparisons'),
        ('errors', 'normalize', 65, 'BINGX-CARDINALITY normalize comparisons'),
    ]:
        test = crate / ('tests/' + target + '.rs')
        oracle = crate / ('tests/' + target + '_oracle.cjs')
        original = oracle.read_text()
        marker = 'process.stdout.write(JSON.stringify('
        assert original.count(marker) == 1
        needle = f'{variable}.len(), {expected_count},'
        test_text = test.read_text()
        assert needle in test_text, needle
        for cut in ['0', '1']:
            prove(f'{target}/{variable}-splice-{cut}/source-count-bypassed', target,
                  {oracle: original.replace(marker, f'{variable}.splice({cut});\n' + marker),
                   test: test_text.replace(needle, f'{variable}.len(), {variable}.len(),')}, assertion)
if selected in (None, 'guard-removal'):
    source = crate / 'src/types.rs'
    test = crate / 'tests/types.rs'
    for name, ts_name in [('FetchBalanceResponse', 'FETCH_BALANCE_RESPONSE'), ('FetchPositionsResponse', 'FETCH_POSITIONS_RESPONSE')]:
        original = source.read_text()
        start = original.index("impl<'de> Deserialize<'de> for " + name + ' {')
        end = original.index('\n}', start)
        part = original[start:end].replace('.map_err(serde::de::Error::custom)',
            '.map(|mut value| { value.data.clear(); value })\n            .map_err(serde::de::Error::custom)')
        changed = original[:start] + part + original[end:]
        test_text = bypass_types(test.read_text())
        line = '            assert_native_cardinality(&native, &case["native"], &format!("{name}/{label}/{route}"));'
        assert test_text.count(line) == 1
        test_text = test_text.replace(line, '            if false { ' + line.strip() + ' }')
        prove(name + '/consuming-empty/all-length-guards-bypassed', 'types',
              {source: changed, test: test_text}, 'native_fields ' + ts_name)
print(f'{completed} named controls restored/passed', flush=True)
