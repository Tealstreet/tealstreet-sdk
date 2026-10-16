"""Compile controlled regressions, require named failures, restore and retest."""
from pathlib import Path
import json
import os
import subprocess

sdk = Path(__file__).resolve().parents[2]
root = sdk.parent
source = sdk / 'safe-cex-venue-lighter/src/ws_private_feed_helpers.rs'
logs = sdk / 'target/lighter-private-feed-mutations'
logs.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
original = source.read_bytes()
controls = [('normalization', 'private_feed_normalization', '(r.trim_upper)(&r.host, (r.to_string)(&r.host, value)?)', '(r.to_string)(&r.host, value)', 'feed::native result status-primary'), ('closed-position', 'private_feed_normalization', 'get(r, &position, "realizedPnl")?', 'get(r, &position, "unrealizedPnl")?', 'feed::native result closed-short'), ('timer', 'private_feed_throttles', 'define(&r, &callback_owner, key, (r.undefined)())?;', 'define(&r, &callback_owner, key, (r.number)(1.0))?;', 'feed::native timer cleared throttleFetchOrders-default'), ('subscribe', 'private_feed_subscribe', 'if auth { "yes" } else { "no" }', 'if auth { "no" } else { "yes" }', 'feed::native calls send-auth'), ('scheduler-override', 'private_feed_throttles', '(r.schedule_call)(\n        r,\n        scheduler,', 'drop(scheduler);\n    schedule_throttled_task(\n        r,\n        owner,', 'feed::native scheduled timer count throttleFetchOrders-override')]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for name, test, before, after, assertion in controls:
    assert original.decode().count(before) == 1, name
    command = [env.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test', '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-venue-lighter', '--jobs', '2', '--test', 'live_private_feed_helpers', '--', test, '--exact']
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
