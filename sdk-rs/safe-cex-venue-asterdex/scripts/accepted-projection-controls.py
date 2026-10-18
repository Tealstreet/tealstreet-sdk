#!/usr/bin/env python3
"""Require isolated compiled production faults and exact passing restores."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
RESULTS = ROOT / 'sdk-rs/target/asterdex-projection-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-projection-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-projection-contracts.mjs']
assert len(os.sys.argv) == 1, 'projection controls take no scope-changing arguments'
CONTROLS = [
    ('nonce-equal-clock-reuse', 'api_helpers.rs',
     'if micros > previous {', 'if micros >= previous {',
     ['api_helpers::build_v3_nonce', 'api_helpers::build_aster_dex_v3_nonce']),
    ('maker-type-inversion', 'builder.rs',
     'opts.r#type == safe_cex_types::OrderType::Limit',
     'opts.r#type != safe_cex_types::OrderType::Limit',
     ['builder::is_aster_maker_only_order', 'builder::get_aster_builder_fee_rate_for_order']),
    ('single-zero-fee-short-circuit', 'builder.rs',
     'if fee_rate_maker == "0" && fee_rate_taker == "0" {',
     'if fee_rate_maker == "0" || fee_rate_taker == "0" {',
     ['builder::resolve_aster_builder_fee_config', 'builder::get_aster_builder_approval_config']),
    ('order-book-sort-inversion', 'exchange_helpers.rs',
     'if descending { order.reverse() } else { order }',
     'if descending { order } else { order.reverse() }',
     ['exchange_helpers::transform_order_book']),
    ('inherited-status-wrong-referent', 'exchange_helpers.rs',
     'TableHit::Mapped(value) => TableHit::Mapped(*value),\n        TableHit::Inherited(member) => TableHit::Inherited(member),',
     'TableHit::Mapped(value) => TableHit::Mapped(*value),\n        TableHit::Inherited(_member) => TableHit::Inherited(safe_cex_num::ProtoMember::ToString),',
     ['exchange_helpers::reverse_map_order_status']),
    ('collector-swallows-coercion-failure', 'exchange_helpers.rs',
     'let value = js_number(item.get("openInterest"))?;',
     'let value = js_number(item.get("openInterest")).unwrap_or(0.0);',
     ['exchange_helpers::collect_open_interest_from_payload']),
    ('order-fallback-negative-zero', 'account_helpers.rs',
     '''fn parse_order_number(order_data: &Value, field: &str) -> Result<f64, JsCoercionError> {
    let value = order_data.get(field);
    if js_truthy(value) {
        Ok(parse_float(&js_string(value)?))
    } else {
        Ok(0.0)
    }
}''',
     '''fn parse_order_number(order_data: &Value, field: &str) -> Result<f64, JsCoercionError> {
    let value = order_data.get(field);
    if js_truthy(value) {
        Ok(parse_float(&js_string(value)?))
    } else {
        Ok(-0.0)
    }
}''',
     ['account_helpers::parse_order_number']),
    ('builder-typed-data-wrong-chain', 'builder.rs',
     'crate::api_helpers::get_aster_dex_aster_chain(testnet)',
     'crate::api_helpers::get_aster_dex_aster_chain(!testnet)',
     ['builder::build_aster_approve_builder_typed_data']),
    ('stp-native-variant-swap', 'exchange_helpers.rs',
     '"EXPIRE_TAKER" => Ok(crate::types::AsterDEXStpMode::V0),',
     '"EXPIRE_TAKER" => Ok(crate::types::AsterDEXStpMode::V1),',
     ['exchange_helpers::parse_stp_mode_response']),
    ('margin-type-inversion', 'exchange_helpers.rs',
     '"marginType": if isolated { "ISOLATED" } else { "CROSSED" }',
     '"marginType": if isolated { "CROSSED" } else { "ISOLATED" }',
     ['exchange_helpers::build_change_margin_type_request']),
]


def run(label, owners=None):
    if RESULTS.exists():
        RESULTS.unlink()
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler error is not evidence; {LOGS}'
    assert RESULTS.exists(), f'{label}: executed native receipts required; {LOGS}'
    rows = json.loads(RESULTS.read_text())
    (LOGS / f'{label}.json').write_text(json.dumps(rows, indent=2) + '\n')
    assert len(rows) == 20 and all(not row.get('classificationOnly') for row in rows), LOGS
    failures = [row for row in rows if row['failures']]
    if owners is None:
        assert result.returncode == 0 and not failures, f'{label}: baseline failure; {LOGS}'
    else:
        assert result.returncode == 1, f'{label}: native assertion exit required; {LOGS}'
        expected = sorted(f'safe_cex_venue_asterdex::{owner}' for owner in owners)
        assert sorted(row['name'] for row in failures) == expected, (
            f'{label}: wrong failed owners {failures}; {LOGS}')
        for row in failures:
            assert any('function output mismatch' in item for item in row['failures']), (
                f'{label}: behavioral mismatch required for {row["name"]}; {LOGS}')
    print(f'{label}: exit {result.returncode}; {len(failures)} failed native owners', flush=True)


run('baseline')
for label, filename, before, after, owners in CONTROLS:
    source = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src' / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, label
    try:
        source.write_text(text.replace(before, after))
        run(label, owners)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{label}-restored')
print(f'Ten isolated compiled faults caught; exact restores pass all twenty owners; logs {LOGS}')
