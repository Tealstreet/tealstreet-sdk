"""Run one Node process per corpus/control; preserve every oracle byte."""
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CASES = [
    ('get_key', 'source_get_key_preserves_null_undefined_and_inherited_values', ['get-key']),
    ('get_key_unknown', 'unknown_rhs_matches_actual_get_key_source', ['unknown-refusals']),
    ('js_bigint', 'bigint_matches_live_node_error_classes', ['bigint']),
    ('js_collation', 'root_collation_matches_live_node', ['collation']),
    ('js_date', 'date_parser_and_time_clip_match_live_node', ['date-parse', 'date-clip']),
    ('js_json_stringify', 'stringify_matches_live_node_bytes', ['stringify']),
    ('js_log10', 'log10_matches_live_node_bits', ['log10']),
    ('js_pow', 'pow_matches_live_node_bits', ['pow']),
    ('js_primitives', 'truthiness_matches_live_node', ['truthy']),
    ('js_primitives', 'number_any_matches_live_node', ['number-any']),
    ('js_property', 'canonical_property_entries_match_live_node', ['property']),
    ('js_table_lookup', 'ordinary_table_hits_match_live_node_branches', ['table', 'table-surrogates', 'table-numeric']),
    ('p4_utils', 'p4_numeric_typescript_differential', ['p4-utils']),
    ('selection', 'selection_typescript_differential', ['selection']),
    ('p4_coercion', 'number_has_its_own_full_string_corpus', ['coercion-number']),
    ('p4_coercion', 'parse_float_has_its_own_prefix_corpus', ['coercion-float']),
    ('p4_coercion', 'parse_int_has_its_own_radix_corpus', ['coercion-int']),
    ('p4_coercion', 'string_is_fallible_and_recursive', ['coercion-string']),
    ('p4_coercion', 'actual_bitget_finite_or_zero_wrapper_preserves_the_primitive', ['coercion-measured']),
    ('lodash_case', 'lodash_compounders_match_pinned_live_package', ['lodash-case']),
]

GRVT_CASES = [
    ('interval', 'interval_matches_actual_source_identity', ['grvt-interval-refusals']),
    ('instrument_symbol', 'instrument_symbol_matches_actual_source', ['grvt-symbol-refusals']),
    ('signing_metadata', 'signing_metadata_matches_actual_source_native_fields', ['grvt-metadata-refusals']),
    ('order_semantics', 'trigger_metadata_matches_actual_source_coercion_and_bits', ['grvt-trigger-boundary']),
]
TRIM_CASES = [('js_trim', 'trim_matches_live_javascript_for_every_unicode_scalar', ['trim-sets', 'trim-members', 'trim-start-members', 'trim-end-members', 'split-members'])]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--group', choices=['num', 'grvt', 'trim'], default='num')
    parser.add_argument('--baseline-only', action='store_true')
    args = parser.parse_args()
    cases = {'num': CASES, 'grvt': GRVT_CASES, 'trim': TRIM_CASES}[args.group]
    expected_targets, expected_names = {'num': (20, 23), 'grvt': (4, 4), 'trim': (1, 5)}[args.group]
    names = [name for row in cases for name in row[2]]
    assert len(cases) == expected_targets, 'cardinality target registry count'
    assert len(names) == len(set(names)) == expected_names, 'cardinality collection registry count'
    package = 'safe-cex-venue-grvt' if args.group == 'grvt' else 'safe-cex-num'
    command = ['cargo', 'test', '--manifest-path', str(ROOT/'sdk-rs/Cargo.toml'), '--locked', '-p', package, '--jobs', '1', '--no-run', '--message-format=json']
    for target in sorted({row[0] for row in cases}): command += ['--test', target]
    build = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE)
    assert build.returncode == 0, 'controls must compile'
    binaries = {}
    for line in build.stdout.splitlines():
        row = json.loads(line)
        if row.get('reason') == 'compiler-artifact' and row.get('executable'):
            binaries[row['target']['name']] = row['executable']
    logs = ROOT/'sdk-rs/target/source-cardinality-controls'
    logs.mkdir(exist_ok=True)
    completed = 0
    for target, test, names in cases:
        command = [binaries[target], '--exact', test, '--include-ignored', '--nocapture']
        baseline = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
        (logs/f'{target}-{test}-baseline.log').write_text(baseline.stdout+baseline.stderr)
        assert baseline.returncode == 0 and '1 passed' in baseline.stdout, (target, test, 'baseline failed')
        if args.baseline_only:
            print(target, test, "baseline passed", flush=True)
            continue
        for name in names:
            for mode in ['empty', 'subset', 'after']:
                result = subprocess.run(command, cwd=ROOT, env={**os.environ, 'SOURCE_CARDINALITY_CONTROL':f'{name}:{mode}'}, capture_output=True, text=True)
                log = result.stdout+result.stderr
                (logs/f'{name}-{mode}.log').write_text(log)
                assertion = f'SOURCE-CARDINALITY {name} {"compared" if mode == "after" else "source"} total'
                assert result.returncode == 101 and assertion in log, (name, mode, result.returncode, log[-1000:])
                completed += 1
                print(name, mode, 101, assertion, flush=True)
        restored = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
        assert restored.returncode == 0 and '1 passed' in restored.stdout, (target, test, 'restored failed')
    expected = 0 if args.baseline_only else expected_names*3
    assert completed == expected, (completed, expected)
    print(f'{len(cases)} baseline targets; {completed} named controls', flush=True)

if __name__ == '__main__': main()
