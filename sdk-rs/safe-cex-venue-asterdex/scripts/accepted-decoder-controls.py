#!/usr/bin/env python3
"""Isolate eleven compiled decoder faults; restore exact source after each."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
RESULTS = ROOT / 'sdk-rs/target/asterdex-decoder-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-decoder-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-decoder-contracts.mjs']
assert len(os.sys.argv) == 1, 'decoder controls take no scope-changing arguments'
CONTROLS = [
    ('check-complete-array-admission', 'builder_approval.rs', 'CheckParams',
     'safe_cex_types::strict_decode::object(d)', 'd', ['builder_approval::CheckParams']),
    ('approve-complete-array-admission', 'builder_approval.rs', 'ApproveParams',
     'safe_cex_types::strict_decode::object(d)', 'd', ['builder_approval::ApproveParams']),
    ('execution-flatten-without-whole-input-capture', 'order_execution.rs', 'ExecutionOrder',
     None, None, ['order_execution::ExecutionOrder']),
    ('filter-limit-negative-zero-loss', 'types.rs', 'AsterDEXMarketResponseFiltersItem',
     'limit: w.limit,', 'limit: w.limit.map(f64::abs),',
     ['types::AsterDEXMarketResponseFiltersItem', 'types::AsterDEXMarketResponse']),
    ('market-delivery-negative-zero-loss', 'types.rs', 'AsterDEXMarketResponse',
     'delivery_date: w.delivery_date,', 'delivery_date: w.delivery_date.map(f64::abs),', ['types::AsterDEXMarketResponse']),
    ('position-time-negative-zero-loss', 'types.rs', 'AsterDEXPositionResponse',
     'update_time: w.update_time,', 'update_time: w.update_time.abs(),', ['types::AsterDEXPositionResponse']),
    ('order-id-negative-zero-loss', 'types.rs', 'AsterDEXOrderResponse',
     'order_id: w.order_id,', 'order_id: w.order_id.abs(),', ['types::AsterDEXOrderResponse']),
    ('ticker-time-negative-zero-loss', 'types.rs', 'AsterDEXTickerResponse',
     'open_time: w.open_time,', 'open_time: w.open_time.abs(),', ['types::AsterDEXTickerResponse']),
    ('kline-tuple-negative-zero-loss', 'types.rs', 'AsterDEXKlineResponse',
     'w.0, w.1,', 'w.0.abs(), w.1,', ['types::AsterDEXKlineResponse']),
    ('websocket-time-negative-zero-loss', 'types.rs', 'AsterDEXWebSocketMessage',
     'capital_e: w.capital_e,', 'capital_e: w.capital_e.abs(),', ['types::AsterDEXWebSocketMessage']),
    ('order-opts-stp-presence-loss', 'types.rs', 'AsterDEXPlaceOrderOpts',
     'stp_mode: w.stp_mode,', 'stp_mode: w.stp_mode.filter(|_| false),',
     ['types::AsterDEXPlaceOrderOpts', 'order_execution::ExecutionOrder']),
]


def run(label, owners=None):
    if RESULTS.exists():
        RESULTS.unlink()
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler failure is not evidence; {LOGS}'
    assert RESULTS.exists(), f'{label}: every native receipt required; {LOGS}'
    rows = json.loads(RESULTS.read_text())
    (LOGS / f'{label}.json').write_text(json.dumps(rows, indent=2) + '\n')
    assert len(rows) == 11 and all(not row.get('classificationOnly') for row in rows), LOGS
    failures = [row for row in rows if row['failures']]
    if owners is None:
        assert result.returncode == 0 and not failures, f'{label}: baseline failure; {LOGS}'
    else:
        expected = sorted(f'safe_cex_venue_asterdex::{owner}' for owner in owners)
        assert result.returncode == 1, f'{label}: compiled assertion failure required; {LOGS}'
        assert sorted(row['name'] for row in failures) == expected, f'{label}: wrong failed owners; {LOGS}'
        assert all(any('native observation mismatch' in item for item in row['failures'])
                   for row in failures), f'{label}: native value/admission mismatch required; {LOGS}'
    print(f'{label}: exit {result.returncode}; {len(failures)} failed native owners', flush=True)


run('baseline')
for label, filename, owner, before, after, owners in CONTROLS:
    source = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src' / filename
    original = source.read_bytes()
    text = original.decode()
    start = text.index(f"impl<'de> serde::Deserialize<'de> for {owner} {{")
    end = text.index('\n}\n', start) + 3
    region = text[start:end]
    if before is None:
        mutant = '''impl<'de> serde::Deserialize<'de> for ExecutionOrder {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = ExecutionOrderWire::deserialize(safe_cex_types::strict_decode::object(d))?;
        let projected = ExecutionOrderOutput { opts: value.opts, post_only: value.post_only };
        Ok(Self { opts: projected.opts, post_only: projected.post_only })
    }
}
'''
    else:
        assert region.count(before) == 1, label
        mutant = region.replace(before, after)
    try:
        source.write_text(text[:start] + mutant + text[end:])
        run(label, owners)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{label}-restored')
print(f'Eleven isolated compiled faults caught; exact restores pass all eleven owners; logs {LOGS}')
