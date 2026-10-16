"""Compiled row and boundary controls; completion requires the named native failure and restore."""
from pathlib import Path
import json
import os
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
LOGS = ROOT / '.aimux/bybit-inverse-reads'
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-bybit/src/inverse_read_surface.rs'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-venue-bybit', '--test', 'inverse_read_surface', '--jobs', '1']
ENV = dict(os.environ, CARGO_INCREMENTAL='1')
original = SOURCE.read_text()
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
subprocess.run(['git', 'diff', '--exit-code'], cwd=ROOT, check=True, capture_output=True)


def function(name):
    matches = re.findall(r'^pub fn ' + name + r'\b.*?^}', original, re.M | re.S)
    assert len(matches) == 1, (name, len(matches))
    return matches[0]


recipes = [
    ('wallet-force', 'fetch_raw_balance', 'h.set(&owner, "forceUnifiedBalance", h.boolean(true))?', 'h.set(&owner, "forceUnifiedBalance", h.boolean(false))?', 'wallet_native_force'),
    ('balance-total', 'format_balance', 'parse(h, &first, "totalEquity")?', 'parse(h, &first, "totalInitialMargin")?', 'balance_native_total'),
    ('orders-limit', 'fetch_orders_helper', 'h.number(50.0)', 'h.number(49.0)', 'orders_native_limit'),
    ('market-alias', 'fetch_markets', 'if h.truthy(&h.apply(some, vec![callback])?)', 'if !h.truthy(&h.apply(some, vec![callback])?)', 'markets_native_original_alias'),
    ('income-arguments', 'fetch_income_history', 'h.apply(fetch, vec![context, opts])', 'h.apply(fetch, vec![context, h.object(vec![("changed", opts)])])', 'income_native_arguments'),
    ('income-range-arguments', 'fetch_income_history_range', 'vec![context, start, end, kind]', 'vec![context, end, start, kind]', 'income_range_native_arguments'),
    ('income-context', 'income_context', 'h.literal("UNIFIED")', 'h.literal("CONTRACT")', 'income_context_native_through_delegate'),
    ('cached-identity', 'cached_api_key_info', 'h.get(owner, "apiKeyInfo")', 'h.get(owner, "store")', 'cache_native_identity'),
    ('balance-strict-length', 'format_balance', 'h.equal(&h.get(&raw, "length")?, &h.number(0.0))', 'h.numeric(&h.get(&raw, "length")?)? == 0.0', 'balance_native_strict_length'),
    ('orders-strict-length', 'fetch_orders_helper', 'h.equal(&h.get(&list, "length")?, &h.number(0.0))', 'h.numeric(&h.get(&list, "length")?)? == 0.0', 'orders_native_strict_length'),
    ('market-iterable-interleave', 'fetch_markets', 'let combined = h.apply(all, vec![requests])?;', 'let values = spread(h, requests)?;\n        let combined = h.apply(all, vec![h.array(values)])?;', 'markets_native_then_before_next'),
    ('market-explicit-receiver', 'fetch_markets', 'let combined = h.apply(all, vec![requests])?;', 'let combined = h.receiver_call(h.object(vec![]), all, vec![requests])?;', 'markets_native_explicit_all_receiver'),
]
controls = []
for name, scope, needle, replacement, assertion in recipes:
    before = function(scope)
    assert before.count(needle) == 1, (name, needle)
    controls.append((name, before, before.replace(needle, replacement), assertion))
before = function('fetch_markets')
selection = '        let all = h.select(h.get(&h.global(), "Promise")?, "all")?;\n'
assert before.count(selection) == 1
changed = before.replace(selection, '').replace('        let requests = h.apply(map, vec![bound])?;\n', '        let requests = h.apply(map, vec![bound])?;\n' + selection)
controls.append(('market-all-capture', before, changed, 'markets_native_boundary_outcome markets-all-capture-before-map'))
assert len(controls) == 13 and len({item[0] for item in controls}) == 13
LOGS.mkdir(parents=True, exist_ok=True)


def run(label, test_name=None):
    command = COMMAND + ([test_name, "--", "--exact"] if test_name else [])
    result = subprocess.run(command, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / ('control-' + label + '.log')).write_text(output)
    return result.returncode, output


def save():
    (LOGS / 'controls.json').write_text(json.dumps(report, indent=2) + '\n')


baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[], attempted=[])
save()
boundary_tests = {
    'balance-strict-length': 'balance_strict_length_boundary',
    'orders-strict-length': 'orders_strict_length_boundary',
    'market-iterable-interleave': 'markets_then_getter_boundary',
    'market-explicit-receiver': 'markets_all_capture_boundary',
    'market-all-capture': 'markets_all_capture_boundary',
}
for name, before, changed, assertion in controls:
    test_name = boundary_tests.get(name, 'source_and_native_inverse_reads')
    record = dict(control=name, assertion=assertion, test=test_name)
    report['attempted'].append(record)
    try:
        assert SOURCE.read_text() == original
        SOURCE.write_text(original.replace(before, changed))
        mutated, output = run(name, test_name)
        record.update(mutation_exit=mutated, compiled_runtime_failure=mutated == 101 and 'test result: FAILED' in output and assertion in output)
    finally:
        SOURCE.write_text(original)
        save()
    if mutated == 0:
        report['survivors'].append(name)
    assert record['compiled_runtime_failure'], (name, output)
    restored, output = run(name + '-restored', test_name)
    record['restored_exit'] = restored
    save()
    assert restored == 0, (name, output)
    report['controls'].append(dict(record))
    save()
    assert SOURCE.read_text() == original
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 13 and not report['survivors']
