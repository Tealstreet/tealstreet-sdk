#!/usr/bin/env python3
"""Serial scoped gates; one compiling discrimination control per new row."""
import argparse
import hashlib
import json
import os
import platform
import subprocess
import tomllib
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--gate-sha', required=True)
parser.add_argument('--base-sha', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
os.chdir(root)
crate = Path('sdk-rs/safe-cex-venue-binance')
sha = subprocess.check_output(['git', 'rev-parse', args.gate_sha], text=True).strip()
base = subprocess.check_output(['git', 'rev-parse', args.base_sha], text=True).strip()
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == sha
subprocess.run(['git', 'merge-base', '--is-ancestor', base, sha], check=True)
logs = Path('sdk-rs/target/binance-cancel-symbol') / sha
logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
inputs = [crate/'src/cancel_symbol_orders.rs', crate/'src/lib.rs', crate/'src/order_logic.rs', crate/'tests/cancel_symbol_orders.rs', crate/'scripts/cancel-symbol-orders.mjs', Path(__file__).relative_to(root)]
inputs += list(Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.exchange.ts.toml'))
inputs += list(Path('packages/safe-cex/src/exchanges/binance').glob('*.exchange.ts'))
inputs += list(Path('packages/safe-cex/src/exchanges/binance').glob('*.types.ts'))
inputs += [Path('packages/safe-cex/src')/p for p in ['types.ts', 'store/store.base.ts', 'utils/clone.ts']]
originals = {p: p.read_bytes() for p in inputs}
for p, content in originals.items():
    assert content == subprocess.check_output(['git', 'show', f'{sha}:{p}']), p
records = []


def run(label, command, exit_code=0, assertion=None):
    result = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs/(label+'.log')).write_bytes(result.stdout)
    output = result.stdout.decode(errors='replace')
    records.append(dict(label=label, gate_sha=sha, command=command, exit=result.returncode, log=str(logs/(label+'.log')), log_sha256=hashlib.sha256(result.stdout).hexdigest(), output=output))
    print(f'{label}: exit {result.returncode}', flush=True)
    assert result.returncode == exit_code, output
    if assertion:
        assert 'Finished `test` profile' in output and assertion in output and 'test result: FAILED. 0 passed; 1 failed;' in output, output
        assert 'left: Closed' in output and 'right: Canceled' in output, output
        records[-1]['named_native_failure'] = assertion
    elif label in ['baseline', 'final-restore']:
        assert 'test result: ok. 4 passed; 0 failed; 0 ignored;' in output, output
    elif label == 'extractor-check':
        assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in output, output


cargo = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-binance', '--test', 'cancel_symbol_orders', '--jobs', '2']
run('baseline', cargo+['--', '--include-ignored'])
source = crate/'src/cancel_symbol_orders.rs'
controls = [
    ('futures-closed-for-canceled', 'cancel_binance_symbol_orders', 'update_status(&regular_orders, OrderStatus::Canceled)', 'futures_cancel_symbol_matches_source', 'binance'),
    ('pm-closed-for-canceled', 'cancel_binance_pm_symbol_orders', '.update_orders_status(&ids, OrderStatus::Canceled)', 'pm_cancel_symbol_matches_source', 'binancepm'),
    ('spot-closed-for-canceled', 'cancel_binance_spot_symbol_orders', '.update_orders_status(&ids, OrderStatus::Canceled)', 'spot_cancel_symbol_matches_source', 'binancespot'),
]
for label, function, before, test, variant in controls:
    original = originals[source].decode()
    start = original.index('pub async fn '+function+'<')
    end = original.find('\n// ts:', start)
    if end < 0:
        end = original.index('\nasync fn all_settled', start)
    body = original[start:end]
    assert body.count(before) == 1
    after = before.replace('OrderStatus::Canceled', 'OrderStatus::Closed')
    try:
        source.write_text(original[:start]+body.replace(before, after)+original[end:])
        assertion = f'CANCEL {variant} case 0 row 0 native status'
        run(label, cargo+['--', '--include-ignored', test, '--exact'], 101, assertion)
        records[-1].update(call_site=str(source)+'::'+function, mutation_before=before, mutation_after=after, native_pair='Closed versus Canceled')
    finally:
        source.write_bytes(originals[source])
    assert source.read_bytes() == originals[source]
run('final-restore', cargo+['--', '--include-ignored'])
run('scoped-cargo-check', ['cargo', 'check', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-binance', '--all-targets', '--jobs', '2'])
run('extractor-check', ['node', '/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js', 'parity:extract:check'])
run('scoped-rustfmt', ['rustfmt', '--edition', '2024', '--check', str(source), str(crate/'tests/cancel_symbol_orders.rs')])
run('git-diff-check', ['git', 'diff', '--check'])
for p, content in originals.items():
    assert p.read_bytes() == content, p


def census(revision):
    rows = {}
    for p in Path('sdk-rs/parity/manifest/exchanges/binance').rglob('*.toml'):
        content = subprocess.check_output(['git', 'show', f'{revision}:{p}']).decode()
        for row in tomllib.loads(content)['symbol']:
            key = (str(p), row['ts'], row['kind'])
            assert key not in rows, key
            rows[key] = row['status']
    return rows


before, after = census(base), census(sha)
assert before.keys() == after.keys()
changed = [dict(manifest=k[0], symbol=k[1], kind=k[2], before=before[k], after=after[k]) for k in before if before[k] != after[k]]
assert len(changed) == 3 and all(r['before'] == 'pending' and r['after'] == 'ported' for r in changed)
counts = lambda rows: {status: list(rows.values()).count(status) for status in ['ported', 'divergent', 'pending']}
evidence = dict(gate_sha=sha, parent_built_on=base, unreviewed_work_in_base=False, moved_pending_to_ported=3, moved_pending_to_divergent=0, census_before=counts(before), census_after=counts(after), changed_rows=changed, records=records, restored_inputs_sha256={str(p): hashlib.sha256(content).hexdigest() for p, content in originals.items()}, controls_surviving=[], enumeration_floor='Completed all 95 authored source scenarios: 43 Futures, 26 PM, 26 Spot, plus eight Futures settlement-order cases. Stopped after three selected compiling status controls, one per newly ported row. No exhaustive input or whole-venue proof claim.', scope='Extracted cancelSymbolOrders bodies at configured DELETE/emitter boundaries with normalized ExchangeError and decoded JSON messages. Futures preserves stable object handles and fixed membership with current IDs at each status update; configured DELETE must start requests on invocation and dropping observation must not cancel them. PM/Spot use live native Store after await. Concrete class/client and continuous Store-handle-view wiring, executable getters, primitive rejection values, object-message identity and emitter mutation of rejected objects remain outside this body boundary.', environment=dict(node=subprocess.check_output(['node', '--version'], text=True).strip(), os=platform.system(), architecture=platform.machine(), typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']), preliminary_uncommitted_runs=[dict(exit=101, tests_ran=False, reason='test imported DefaultStore from root instead of store module; fixed import'), dict(exit=0, passed=4, failed=0, ignored=0, scope='before final source-trace cardinality assertions; not substituted for committed gates')], manifest_writer=dict(command=['node', '/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js', 'parity:extract'], exit=0, output='Updated 3 manifests; 0 added, 0 orphaned symbols.', scope='uncommitted row edits before candidate commit'), not_run=['full crate test suite', 'workspace compilation', 'host clippy', 'wasm clippy', 'CI reruns', 'concrete class/client/continuous handle composition', 'whole-venue audit', 'cross-engine error wording verification'])
(crate/'CANCEL_SYMBOL_GATE_EXITS.json').write_text(json.dumps(evidence, indent=2)+'\n')
print('Evidence recorded at '+str(crate/'CANCEL_SYMBOL_GATE_EXITS.json'), flush=True)
