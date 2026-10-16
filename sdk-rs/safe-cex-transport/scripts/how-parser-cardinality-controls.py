"""Prove the parser corpus guards independently; restore source after every run."""
import os
import signal
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-transport/src/how.rs'
ORIGINAL = SOURCE.read_text()
TARGET = 'how::wire_tests::raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace'
ENV = dict(os.environ)
ENV['CARGO_BUILD_JOBS'] = '1'
ENV.setdefault('SAFE_CEX_TYPESCRIPT_PATH', str(ROOT / 'node_modules/typescript'))
ENV.pop('P2B_JSON_PARSE_CORRECTION_CONTROL', None)
ENV.pop('P2B_JSON_PARSE_CARDINALITY_CONTROL', None)
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '--locked', '--offline', '-p', 'safe-cex-transport', '--lib']


def replace_once(source, old, new):
    assert source.count(old) == 1, f'mutation anchor count: {old}'
    return source.replace(old, new, 1)


def run(label, control, expected_code, assertion=None, all_parser_tests=False):
    env = dict(ENV)
    if control:
        env['P2B_JSON_PARSE_CARDINALITY_CONTROL'] = control
    args = ['how::wire_tests::raw_parser'] if all_parser_tests else [TARGET, '--', '--exact', '--nocapture']
    result = subprocess.run(COMMAND + args, cwd=ROOT, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    assert result.returncode == expected_code, f'{label}: {result.returncode}\n{output}'
    if assertion:
        assert assertion in output and 'test result: FAILED' in output, f'{label}: wrong failure\n{output}'
    if control:
        size = {'empty': 0, 'subset': 9, 'extra': 11}[control]
        assert f'HOW-PARSER mutation applied {control} {size}' in output, output
    print(f'{label}: exit {result.returncode}' + (f', {assertion}' if assertion else ''), flush=True)


def interrupted(_signal, _frame):
    raise KeyboardInterrupt('cardinality controls interrupted; restoring source')


signal.signal(signal.SIGTERM, interrupted)


source_guard = '        assert_eq!(cases.len(), 10, "HOW-PARSER source case count");\n'
without_source_guard = replace_once(ORIGINAL, source_guard, '')
fixed_walk = '''        for index in 0..10 {
            let case = cases
                .get(index)
                .expect("HOW-PARSER missing independently required case");'''
without_guards = replace_once(without_source_guard, fixed_walk, '        for case in cases {')
without_guards = replace_once(without_guards, '        assert_eq!(comparisons, 30, "HOW-PARSER native comparison count");\n', '')
without_guards = replace_once(without_guards, '''        assert_eq!(
            cases.len() * 3,
            comparisons,
            "HOW-PARSER source/native case coverage"
        );''', '        assert_eq!(comparisons, cases.len() * 3);')
try:
    SOURCE.write_text(without_source_guard)
    for control in ('empty', 'subset', 'extra'):
        assertion = 'HOW-PARSER source/native case coverage' if control == 'extra' else 'HOW-PARSER missing independently required case'
        run(f'source-guard-bypass/{control}', control, 101, assertion)
    SOURCE.write_text(without_guards)
    for control in ('empty', 'subset', 'extra'):
        run(f'all-independent-guards-bypassed/{control}', control, 0)
finally:
    SOURCE.write_text(ORIGINAL)
run('restored-parser-targets', None, 0, all_parser_tests=True)
