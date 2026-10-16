#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess

CRATE = Path(__file__).resolve().parents[1]
CARGO = shutil.which('cargo')


def consumer_mutation(source, target, operation):
    declaration = '    let cases: Vec<Case> ='
    source = source.replace(declaration, '    let mut cases: Vec<Case> =', 1)
    marker = '    let admitted: Vec<Case> ='
    actions = {
        'empty': 'cases.clear();',
        'subset': 'cases.pop();',
        'reverse': 'cases.reverse();',
        'replace': ('cases[1].raw = cases[0].raw.clone();' if target == 'record' else 'cases[0].rejected[0] = "null".to_owned();'),
    }
    assert marker in source
    return source.replace(marker, '    ' + actions[operation] + '\n' + marker, 1)


def bypass_source_dimensions(source, target):
    marker = '    assert_eq!(cases.len(), '
    start = source.index(marker)
    end = source.index('    let mut comparisons = ', start)
    source = source[:start] + source[end:]
    if target == 'literal':
        start = source.index('        assert_eq!(\n            (case.accepted.len(), case.rejected.len())')
        end = source.index('        macro_rules! dispatch', start)
        source = source[:start] + source[end:]
    return source


def bypass_content(source):
    source, removed = re.subn(r'        assert_eq!\(\s*case,\s*admitted\.get\(index\)\.unwrap\(\),\s*"(?:RECORD|LITERAL)-CONTENT native input \{index\}"\s*\);\n', '', source)
    assert removed == 1
    source, removed = re.subn(r'    let admitted: Vec<Case> =\s*serde_json::from_str\(include_str!\("fixtures/(?:record-shapes|literal-unions)\.json"\)\)\.unwrap\(\);\n', '', source)
    assert removed == 1
    return source


def controls(source, target):
    prefix = target.upper()
    result = []
    for operation in ['empty', 'subset']:
        changed = consumer_mutation(source, target, operation)
        result.append((operation, changed, 101, f'{prefix}-CARDINALITY source roots'))
        result.append((operation + '-source-counts-bypassed', bypass_source_dimensions(changed, target), 101, f'{prefix}-CARDINALITY native index coverage'))
    for operation in ['replace', 'reverse']:
        changed = consumer_mutation(source, target, operation)
        result.append((operation, changed, 101, f'{prefix}-CONTENT native input'))
    if target == 'record':
        changed = source.replace('serde_json::from_str::<T>(&case.raw).is_ok()', 'case.raw.is_empty()', 1)
        assert changed != source
        result.append(('incorrect-accepted-receipt', changed, 101, 'RECORD-NATIVE decode receipts'))
        result.append(('replace-content-bypassed', bypass_content(consumer_mutation(source, target, 'replace')), 101, 'RECORD-NATIVE decode receipts'))
    else:
        changed = bypass_content(consumer_mutation(source, target, 'replace'))
        result.append(('replace-content-bypassed', changed, 0, 'test result: ok. 1 passed'))
        # Keep the original same-rejection survivor. Cross the source accepted-set
        # boundary separately: empty string/null both reject; unifiedAccount accepts.
        changed = changed.replace('cases[0].rejected[0] = "null".to_owned();', 'cases[0].rejected[0] = r#""unifiedAccount""#.to_owned();', 1)
        result.append(('replace-with-accepted-content-bypassed', changed, 101, 'LITERAL-NATIVE rejected result HyperliquidUserAbstractionMode'))
        for operation, edit in [('empty-native', 'accepted.clear(); rejected.clear(); qualified.clear();'), ('subset-native', 'rejected.pop();'), ('extra-native', 'accepted.push(true);')]:
            changed = source.replace('let (accepted, rejected, qualified) = dispatch!', 'let (mut accepted, mut rejected, mut qualified) = dispatch!', 1)
            changed = changed.replace('        assert_eq!(\n            accepted.len(),', '        ' + edit + '\n        assert_eq!(\n            accepted.len(),', 1)
            assert changed != source
            # All three mutable receipts are deliberately used to keep controls warning-free.
            changed = changed.replace('        ' + edit, '        accepted.reserve(0); rejected.reserve(0); qualified.reserve(0);\n        ' + edit, 1)
            assertion = 'accepted' if operation != 'subset-native' else 'rejected'
            result.append((operation, changed, 101, f'LITERAL-NATIVE {assertion} receipt count'))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', choices=['record', 'literal'], required=True)
    parser.add_argument('--only', nargs='*')
    args = parser.parse_args()
    target = 'record_shapes' if args.target == 'record' else 'literal_unions'
    function = 'every_record_is_map_only' if args.target == 'record' else 'actual_serde_literal_sets_match_ts_and_reject_widening'
    file = CRATE / 'tests' / (target + '.rs')
    original = file.read_text()
    cases = controls(original, args.target)
    expected = (
        'empty', 'empty-source-counts-bypassed',
        'subset', 'subset-source-counts-bypassed', 'replace', 'reverse',
        'incorrect-accepted-receipt', 'replace-content-bypassed',
    ) if args.target == 'record' else (
        'empty', 'empty-source-counts-bypassed',
        'subset', 'subset-source-counts-bypassed', 'replace', 'reverse',
        'replace-content-bypassed', 'replace-with-accepted-content-bypassed', 'empty-native', 'subset-native', 'extra-native',
    )
    labels = tuple(case[0] for case in cases)
    assert len(set(labels)) == len(labels), 'accepted-set control uniqueness'
    assert labels == expected, 'accepted-set exact control inventory'
    selected = [case for case in cases if not args.only or case[0] in args.only]
    assert selected and (not args.only or len(selected) == len(args.only)), 'unknown control name'
    assert CARGO, 'cargo must be available on PATH'
    command = [CARGO, 'test', '--manifest-path', str(CRATE.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-hyperliquid', '--test', target, function, '--locked', '--offline', '--jobs', '2', '--', '--exact']
    env = dict(os.environ, CARGO_BUILD_JOBS='1')
    try:
        for label, changed, expected_exit, assertion in selected:
            file.write_text(changed)
            path = Path('/tmp') / f'hl-accepted-set-{args.target}-{label}.log'
            with path.open('w') as log:
                completed = subprocess.run(command, cwd=CRATE.parents[1], env=env, stdout=log, stderr=subprocess.STDOUT)
            output = path.read_text()
            assert completed.returncode == expected_exit, f'{label}: exit {completed.returncode}; see {path}'
            assert 'running 1 test' in output and assertion in output, f'{label}: wrong failure, see {path}'
            if expected_exit == 101:
                assert 'test result: FAILED.' in output, f'{label}: compile-only failure is not proof'
            print(f'{args.target}/{label}: exit {completed.returncode}: {assertion}', flush=True)
            if expected_exit == 0:
                print('FINDING: HyperliquidUserAbstractionMode empty-string -> null survives at literal_unions.rs::actual_serde_literal_sets_match_ts_and_reject_widening with content guard bypassed; actual strict TypeScript checker rejects both (2322). Acceptance is equivalent; values and diagnostic messages differ. See literal-rejection-equivalence.cjs.', flush=True)
            file.write_text(original)
    finally:
        file.write_text(original)
    # Restoration is a measured gate, not inferred from the finally block.
    path = Path('/tmp') / f'hl-accepted-set-{args.target}-restored.log'
    with path.open('w') as log:
        completed = subprocess.run(command, cwd=CRATE.parents[1], env=env, stdout=log, stderr=subprocess.STDOUT)
    output = path.read_text()
    assert completed.returncode == 0 and 'test result: ok. 1 passed' in output, f'restored: see {path}'
    print(f'{args.target}/restored: exit 0: test result: ok. 1 passed', flush=True)


if __name__ == '__main__':
    main()
