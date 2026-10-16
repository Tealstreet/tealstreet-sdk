#!/usr/bin/env python3
"""Check census provenance and arithmetic. This does not run native or TS code."""
from collections import Counter
import hashlib
import functools
import json
from pathlib import Path
import re
import subprocess
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
EXPECTED = {'okx': 26, 'kraken': 31, 'weex': 35,
            'polymarket': 18, 'mexc': 40, 'asterdex': 92}


def require(condition, label):
    if not condition:
        raise AssertionError(label)


@functools.cache
def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


def load(name):
    return json.loads((HERE / name).read_text())


def verify():
    selection, rows, summary = (load(n) for n in
                                ['selection.json', 'rows.json', 'summary.json'])
    require(len(rows) == 242, 'independent_242_row_floor')
    require(set(selection['venues']) == set(EXPECTED), 'six_venue_floor')
    identities = [(r['source'], r['ts']) for r in rows]
    require(len(set(identities)) == 242, 'no_overlapping_row_double_count')
    paired = direct = partial = 0
    for venue, expected in EXPECTED.items():
        frozen = selection['venues'][venue]
        names = git('ls-tree', '-r', '--name-only', frozen['input_sha'],
                    f'sdk-rs/parity/manifest/exchanges/{venue}').decode().splitlines()
        native_manifest = {}
        for name in names:
            if not name.endswith('.toml'):
                continue
            doc = tomllib.loads(git('show', frozen['input_sha'] + ':' + name).decode())
            for symbol in doc.get('symbol', []):
                if symbol['status'] == 'divergent':
                    key = (doc['file']['ts'], symbol['ts'])
                    require(key not in native_manifest, 'unique_frozen_manifest_identity')
                    native_manifest[key] = (name, symbol['rs'])
        require(len(native_manifest) == expected, f'{venue}_independent_manifest_floor')
        subset = [r for r in rows if r['venue'] == venue]
        require(len(subset) == expected, f'{venue}_ledger_floor')
        require({(r['source'], r['ts']) for r in subset} == set(native_manifest),
                f'{venue}_complete_frozen_divergent_identity_set')
        require([r['iteration'] for r in subset] == list(range(1, expected + 1)),
                f'{venue}_finite_iterations')
        require(subset[-1]['ts'] == frozen['stop'], f'{venue}_declared_stop')
        require([(r['source'], r['ts'], r['rs']) for r in subset] ==
                [(r['source'], r['ts'], r['rs']) for r in frozen['rows']],
                f'{venue}_frozen_selection_order_and_mapping')
        detail = load(f'details/{venue}.json')
        details = detail if isinstance(detail, list) else detail['rows']
        require(len(details) == expected, f'{venue}_detail_floor')
        for row, original in zip(subset, details):
            require(row['evidence_level'] in ['paired-runtime-attested',
                    'paired-runtime-prose-pass', 'test-code-only', 'source-only', 'unknown'],
                    'allowed_evidence_level')
            require(row['some_direct_native_assertions'] is True or
                    row['some_direct_native_assertions'] is False or
                    row['some_direct_native_assertions'] in ['partial', 'unknown'],
                    'allowed_native_assertion_axis')
            require((row['source'], row['ts']) ==
                    (original['source'], original['ts']), 'detail_identity')
            require(row['evidence_level'] ==
                    original.get('evidence_level', original.get('level')), 'detail_evidence_level')
            require(row['input_sha'] == original.get('input_sha'), 'detail_input_sha')
            name, targets = native_manifest[(row['source'], row['ts'])]
            require(row['manifest'] == name and row['rs'] == targets, 'exact_frozen_mapping')
            require(row['whole_row_parity_established'] is False and
                    row['current_head_runtime_rerun_by_this_census'] is False,
                    'no_full_parity_or_current_head_inference')
            strict = row['evidence_level'] == 'paired-runtime-attested'
            if not strict and row['evidence_level'] != 'paired-runtime-prose-pass':
                continue
            require(isinstance(row['input_sha'], str) and
                    re.fullmatch('[0-9a-f]{40}', row['input_sha']) and
                    (row['exit'] == 0 if strict else row['exit'] is None) and
                    row['receipt'] and row['test_selectors'],
                    'paired_runtime_receipt')
            require(row['test_file'] and row['source_oracle'], 'paired_test_and_source_oracle')
            require(row['receipt_artifacts'], 'paired_receipt_artifact_pins')
            for artifact in row['receipt_artifacts']:
                raw = git('show', artifact['snapshot_sha'] + ':' + artifact['path'])
                require(hashlib.sha256(raw).hexdigest() == artifact['sha256'],
                        'historical_receipt_content_hash')
            for key in ['test_file', 'source_oracle']:
                raw = git('show', row['input_sha'] + ':' + row[key])
                require(hashlib.sha256(raw).hexdigest() == row[key + '_sha256_at_input'],
                        'historical_blob_hash_' + key)
            require([a['selector'] for a in row['selector_artifacts']] == row['test_selectors'],
                    f'selector_artifact_floor::{venue}::{row["iteration"]}')
            for artifact in row['selector_artifacts']:
                raw = git('show', row['input_sha'] + ':' + artifact['path'])
                require(hashlib.sha256(raw).hexdigest() == artifact['sha256'],
                        'historical_selector_file_hash')
                selector = artifact['selector'].split('::')[-1]
                require(re.search(r'\bfn\s+' + re.escape(selector) + r'\s*\(', raw.decode()),
                        f'historical_selector_exists::{venue}::{row["iteration"]}::{selector}')
            if not strict:
                continue
            paired += 1
            direct += row['some_direct_native_assertions'] is True
            partial += row['some_direct_native_assertions'] == 'partial'
        actual = Counter(r['evidence_level'] for r in subset)
        expected_summary = summary['venues'][venue]
        require(expected_summary['denominator'] == expected and
                expected_summary['levels'] == dict(actual), 'summary_venue_levels')
        require(expected_summary['paired_runtime_attested'] == actual['paired-runtime-attested'],
                'summary_venue_paired')
        require(expected_summary['paired_with_some_direct_native_assertions'] ==
                sum(r['evidence_level'] == 'paired-runtime-attested' and
                    r['some_direct_native_assertions'] is True for r in subset),
                'summary_venue_direct_native')
        require(expected_summary['paired_with_partial_native_assertions'] ==
                sum(r['evidence_level'] == 'paired-runtime-attested' and
                    r['some_direct_native_assertions'] == 'partial' for r in subset),
                'summary_venue_partial_native')
    require(summary['historical_paired_runtime_attested'] == paired, 'summary_paired_total')
    require(summary['paired_with_some_direct_native_assertions'] == direct,
            'summary_direct_native_subset_total')
    require(summary['paired_with_partial_native_assertions'] == partial,
            'summary_partial_native_subset_total')
    require(summary['whole_row_parity_established'] == 0 and
            summary['current_head_runtime_rerun_by_this_census'] == 0 and
            summary['status_rows_moved'] == 0, 'no_unmeasured_scope_claim')
    require(not git('diff', summary['authoring_parent'], '--', 'sdk-rs/parity/manifest'),
            'zero_manifest_status_or_annotation_changes')
    changed = git('diff', '--name-only', summary['authoring_parent']).decode().splitlines()
    require(all(name.startswith('sdk-rs/parity/evidence/behavior-confidence-census/')
                for name in changed), 'evidence_only_tracked_diff')
    for path in HERE.rglob('*.json'):
        require(not re.search(r'/(?:Users|home)/|/opt/homebrew/', path.read_text()),
                'no_machine_paths_' + path.name)
    print(f'PASS metadata only: 242 unique frozen rows; {paired} historically paired; '
          f'{direct} with some direct native assertions; {partial} partial. '
          'No native/TS execution or whole-row parity inferred.')


if __name__ == '__main__':
    verify()
