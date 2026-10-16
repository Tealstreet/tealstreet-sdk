"""Compile controlled regressions, require named failures, restore and retest."""
from pathlib import Path
import json
import os
import subprocess

sdk = Path(__file__).resolve().parents[2]
root = sdk.parent
source = sdk / 'safe-cex-venue-lighter/src/ws_private_stream_handlers.rs'
logs = sdk / 'target/lighter-private-stream-mutations'
logs.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
original = source.read_bytes()
controls = [('dispatch', 'private_stream_dispatch', 'if sell { "SELL" } else { "BUY" }', 'if sell { "BUY" } else { "SELL" }', 'stream::native calls handleAccountAllTopic-all'), ('orders', 'private_stream_orders', '(r.feed.number)(500.0)', '(r.feed.number)(501.0)', 'stream::native calls handleOrderTopic-empty'), ('positions', 'private_stream_positions_fills', 'vec![(r.feed.array)(vec![mapped.clone(), mapped])]', 'vec![(r.feed.array)(vec![(r.feed.undefined)(), mapped])]', 'stream::native retained identities handlePositionTopic-mapped'), ('fills', 'private_stream_positions_fills', '(r.feed.number)(250.0)', '(r.feed.number)(251.0)', 'stream::native calls handleFillsTopic-readonly-empty'), ('ping', 'private_stream_ping_open', '(r.less_than)(&r.feed.host, elapsed, interval)?', '(r.less_equal)(&r.feed.host, elapsed, interval)?', 'stream::native state ping-rate-boundary'), ('open', 'private_stream_ping_open', '("hasReceivedAccountAllOrdersAck", (r.feed.boolean)(false))', '("hasReceivedAccountAllOrdersAck", (r.feed.boolean)(true))', 'stream::native state onOpen-basic'), ('auth', 'private_stream_auth', '(r.feed.number)(60.0)', '(r.feed.number)(61.0)', 'stream::native before-await calls authenticateAndSubscribe-success')]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for name, test, before, after, assertion in controls:
    assert original.decode().count(before) == 1, name
    command = [env.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test', '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-venue-lighter', '--jobs', '2', '--test', 'live_private_stream_handlers', '--', test, '--exact']
    try:
        source.write_text(original.decode().replace(before, after))
        failed = subprocess.run(command, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (logs / f'{name}-broken.log').write_text(failed.stdout)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (logs / f'{name}-restored.log').write_text(restored.stdout)
    result = dict(control=name, sha=sha, test=test, assertion=assertion, broken=failed.returncode, restored=restored.returncode, caught=assertion in failed.stdout and 'assertion `left == right` failed' in failed.stdout and 'test result: FAILED' in failed.stdout and 'error[E' not in failed.stdout and failed.returncode == 101)
    results.append(result)
    (logs / 'results.json').write_text(json.dumps(results, indent=2))
    print(json.dumps(result), flush=True)
    assert result['caught'], f'Mutation survivor or invalid failure: {name}; see {logs}'
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, name
assert source.read_bytes() == original
