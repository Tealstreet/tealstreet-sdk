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
logs = Path('sdk-rs/target/binance-delegated-methods') / sha
logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
inputs = [crate/'src/delegated_methods.rs', crate/'src/lib.rs', crate/'tests/delegated_methods.rs', crate/'scripts/delegated-methods.mjs', Path(__file__).relative_to(root)]
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
        assert 'left:' in output and 'right:' in output, output
        records[-1]['named_native_failure'] = assertion
    elif label in ['baseline', 'final-restore']:
        assert 'test result: ok. 7 passed; 0 failed; 0 ignored;' in output, output
    elif label == 'extractor-check':
        assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in output, output


cargo = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-binance', '--test', 'delegated_methods', '--jobs', '2']
run('baseline', cargo+['--', '--include-ignored'])
source = crate/'src/delegated_methods.rs'
controls = [
    ('pm-options-copy', 'listen_binance_pm_ohlcv', 'listen(opts, callback)', 'listen(Rc::new(RefCell::new(opts.borrow().clone())), callback)', 'pm_candles', 'DELEGATE binancepm listenOHLCV native options identity', 'false versus true'),
    ('spot-options-copy', 'listen_binance_spot_ohlcv', 'listen(opts, callback)', 'listen(Rc::new(RefCell::new(opts.borrow().clone())), callback)', 'spot_candles', 'DELEGATE binancespot listenOHLCV native options identity', 'false versus true'),
    ('futures-provider-flip', 'listen_binance_order_book', 'if has_external_provider {', 'if !has_external_provider {', 'futures_book', 'DELEGATE binance listenOrderBook native provider route', 'external versus public'),
    ('pm-wrong-symbol', 'listen_binance_pm_order_book', 'listen(symbol, callback)', 'listen(if symbol == "BTCUSDT" { "WRONG" } else { symbol }, callback)', 'pm_book', 'DELEGATE binancepm listenOrderBook native symbol', 'WRONG versus BTCUSDT'),
    ('spot-wrong-symbol', 'listen_binance_spot_order_book', 'listen(symbol, callback)', 'listen(if symbol == "BTCUSDT" { "WRONG" } else { symbol }, callback)', 'spot_book', 'DELEGATE binancespot listenOrderBook native symbol', 'WRONG versus BTCUSDT'),
    ('history-not-symbol-scoped', 'fetch_binance_historical_positions', 'fold(opts, true)?.await', 'fold(opts, false)?.await', 'historical_positions', 'DELEGATE history native symbolScoped', 'false versus true'),
]
for label, function, before, after, test, assertion, pair in controls:
    original = originals[source].decode()
    prefix = 'pub async fn ' if function == 'fetch_binance_historical_positions' else 'pub fn '
    start = original.index(prefix+function+'<')
    end = original.find('\n// ts:', start)
    if end < 0:
        end = len(original)
    body = original[start:end]
    assert body.count(before) == 1
    try:
        source.write_text(original[:start]+body.replace(before, after)+original[end:])
        run(label, cargo+['--', '--include-ignored', test, '--exact'], 101, assertion)
        records[-1].update(call_site=str(source)+'::'+function, mutation_before=before, mutation_after=after, native_pair=pair)
    finally:
        source.write_bytes(originals[source])
    assert source.read_bytes() == originals[source]
run('final-restore', cargo+['--', '--include-ignored'])
run('scoped-cargo-check', ['cargo', 'check', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-binance', '--all-targets', '--jobs', '2'])
run('extractor-check', ['node', '/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js', 'parity:extract:check'])
run('scoped-rustfmt', ['rustfmt', '--edition', '2024', '--check', str(source), str(crate/'tests/delegated_methods.rs')])
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
assert len(changed) == 6 and all(r['before'] == 'pending' and r['after'] == 'ported' for r in changed)
counts = lambda rows: {status: list(rows.values()).count(status) for status in ['ported', 'divergent', 'pending']}
evidence = dict(gate_sha=sha, parent_built_on=base, unreviewed_work_in_base=False, moved_pending_to_ported=6, moved_pending_to_divergent=0, census_before=counts(before), census_after=counts(after), changed_rows=changed, records=records, restored_inputs_sha256={str(p): hashlib.sha256(content).hexdigest() for p, content in originals.items()}, controls_surviving=[], enumeration_floor='Completed all 27 authored source cases: Futures book 8 (stop external=true/cleanup-reject), history 3 (stop reject), PM/Spot candle/book 4 each (stop cleanup-reject), plus one pending-fold case. Stopped after six selected compiling controls, one per new row. No exhaustive input or whole-venue proof claim.', scope='Five listener bodies and one historical-position delegation at configured method boundaries. Typed decoded OHLCVOptions handle preserves object identity; generic callbacks/results/errors are forwarded unchanged. Concrete class/provider/websocket/journal bindings, property getters, malformed providers and arrow receiver composition remain pending. Futures seeded listenOHLCV is not in this slice.', environment=dict(node=subprocess.check_output(['node', '--version'], text=True).strip(), os=platform.system(), architecture=platform.machine(), typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']), preliminary_uncommitted_runs=[dict(exit=101, tests_ran=False, reason='static fixture callback borrowed case mode string; captured boolean flags instead'), dict(exit=0, passed=7, failed=0, ignored=0, scope='before final interval/total/last-side assertion additions; not substituted for committed gates')], manifest_writer=dict(command=['node', '/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js', 'parity:extract'], exit=0, output='Updated 3 manifests; 0 added, 0 orphaned symbols.', scope='uncommitted row edits before candidate commit'), not_run=['full crate test suite', 'workspace compilation', 'host clippy', 'wasm clippy', 'CI reruns', 'concrete class/provider/websocket/journal composition', 'whole-venue audit', 'cross-engine error wording verification'])
evidence['earlier_partial_runner'] = {'gate_sha': '6b4f5622cba8fb03e569842b849e09846c0fd14a', 'runner_exit': 1, 'records': [{'label': 'baseline', 'gate_sha': '6b4f5622cba8fb03e569842b849e09846c0fd14a', 'exit': 0, 'compiled': True, 'log_sha256': 'da925baa552aa6a7dd0c2c446c7980bdebc277cb4eeb1fa870b64fa396798d7c', 'output': '   Compiling safe-cex-venue-binance v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-venue-binance/sdk-rs/safe-cex-venue-binance)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 16.28s\n     Running tests/delegated_methods.rs (sdk-rs/target/debug/deps/delegated_methods-d74817607bec992f)\n\nrunning 7 tests\ntest historical_positions_waits_without_retry ... ok\ntest pm_book ... ok\ntest spot_candles ... ok\ntest pm_candles ... ok\ntest historical_positions ... ok\ntest futures_book ... ok\ntest spot_book ... ok\n\ntest result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s\n\n'}, {'label': 'pm-options-copy', 'gate_sha': '6b4f5622cba8fb03e569842b849e09846c0fd14a', 'exit': 101, 'compiled': True, 'log_sha256': '6724ea18925531aaddf6464fa5e65447a48a428790461edde16f4bbb4497851e', 'output': "   Compiling safe-cex-venue-binance v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-venue-binance/sdk-rs/safe-cex-venue-binance)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.79s\n     Running tests/delegated_methods.rs (sdk-rs/target/debug/deps/delegated_methods-d74817607bec992f)\n\nrunning 1 test\ntest pm_candles ... FAILED\n\nfailures:\n\n---- pm_candles stdout ----\n\nthread 'pm_candles' (15083729) panicked at safe-cex-venue-binance/tests/delegated_methods.rs:206:13:\nassertion `left == right` failed: DELEGATE binancepm listenOHLCV native options identity\n  left: false\n right: true\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\nfailures:\n    pm_candles\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.23s\n\nerror: test failed, to rerun pass `-p safe-cex-venue-binance --test delegated_methods`\n"}, {'label': 'spot-options-copy', 'gate_sha': '6b4f5622cba8fb03e569842b849e09846c0fd14a', 'exit': 101, 'compiled': True, 'log_sha256': '7b4da103cfbd84a03b1ac52768619b2d795e358fd1583089f11392a6a821932e', 'output': "   Compiling safe-cex-venue-binance v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-venue-binance/sdk-rs/safe-cex-venue-binance)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.90s\n     Running tests/delegated_methods.rs (sdk-rs/target/debug/deps/delegated_methods-d74817607bec992f)\n\nrunning 1 test\ntest spot_candles ... FAILED\n\nfailures:\n\n---- spot_candles stdout ----\n\nthread 'spot_candles' (15087213) panicked at safe-cex-venue-binance/tests/delegated_methods.rs:206:13:\nassertion `left == right` failed: DELEGATE binancespot listenOHLCV native options identity\n  left: false\n right: true\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\nfailures:\n    spot_candles\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.22s\n\nerror: test failed, to rerun pass `-p safe-cex-venue-binance --test delegated_methods`\n"}, {'label': 'futures-provider-flip', 'gate_sha': '6b4f5622cba8fb03e569842b849e09846c0fd14a', 'exit': 101, 'compiled': True, 'log_sha256': '6d6d7f8c66751b325d649b88b94520f0a7bc22b6313fdb2f079071075fec83b3', 'output': '   Compiling safe-cex-venue-binance v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-venue-binance/sdk-rs/safe-cex-venue-binance)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.57s\n     Running tests/delegated_methods.rs (sdk-rs/target/debug/deps/delegated_methods-d74817607bec992f)\n\nrunning 1 test\ntest futures_book ... FAILED\n\nfailures:\n\n---- futures_book stdout ----\n\nthread \'futures_book\' (15088558) panicked at safe-cex-venue-binance/tests/delegated_methods.rs:194:9:\nassertion `left == right` failed: DELEGATE binance listenOrderBook native provider route\n  left: "external"\n right: "public"\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\nfailures:\n    futures_book\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.21s\n\nerror: test failed, to rerun pass `-p safe-cex-venue-binance --test delegated_methods`\n'}, {'label': 'pm-wrong-symbol', 'gate_sha': '6b4f5622cba8fb03e569842b849e09846c0fd14a', 'exit': 101, 'compiled': False, 'log_sha256': '8656bbdd1753232f9b5bcd65027ce476aa92c5bb9c268ec4e3a38ef8c30b5172', 'output': '   Compiling safe-cex-venue-binance v0.1.0 (/Users/sam/cs/tealstreet-next/.aimux/worktrees/rust-venue-binance/sdk-rs/safe-cex-venue-binance)\nerror: unused variable: `symbol`\n  --> safe-cex-venue-binance/src/delegated_methods.rs:44:5\n   |\n44 |     symbol: &str,\n   |     ^^^^^^ help: if this is intentional, prefix it with an underscore: `_symbol`\n   |\n   = note: `-D unused-variables` implied by `-D warnings`\n   = help: to override `-D warnings` add `#[allow(unused_variables)]`\n\nerror: could not compile `safe-cex-venue-binance` (lib) due to 1 previous error\n'}], 'invalid_control_call_site': 'sdk-rs/safe-cex-venue-binance/src/delegated_methods.rs::listen_binance_pm_order_book', 'reason': 'Wrong-symbol control discarded symbol, rejected by -D unused-variables before test; not a killed mutation or survivor. Corrected mutation still references symbol. Source restored exactly in finally.', 'not_reached': ['spot wrong-symbol control', 'history symbolScoped control', 'final-restore', 'cargo check', 'extractor', 'Rustfmt', 'diff check']}
evidence['initial_runner_abort'] = dict(gate_sha=subprocess.check_output(['git', 'rev-parse', '1230bc6184'], text=True).strip(), exit=1, gates_ran=False, reason='Copied runner oracle filename had an extra -orders suffix; corrected before any gates or mutations ran.')
(crate/'DELEGATED_METHODS_GATE_EXITS.json').write_text(json.dumps(evidence, indent=2)+'\n')
print('Evidence recorded at '+str(crate/'DELEGATED_METHODS_GATE_EXITS.json'), flush=True)
