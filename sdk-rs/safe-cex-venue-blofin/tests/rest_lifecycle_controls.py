"""Serial deliberate mutations: compile rejection is not assertion evidence."""
import json
import os
import pathlib
import subprocess
import sys

root = pathlib.Path.cwd()
out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
begin = int(sys.argv[2]) if len(sys.argv) > 2 else 0
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
prefix = 'sdk-rs/safe-cex-venue-blofin/src/'
controls = [
    ('api.rs', 'signed_key', 'native_create_api', '("ACCESS-KEY", self.options.key.clone())', '("ACCESS-KEY", "wrong-key".into())'),
    ('rest_lifecycle.rs', 'constructor_assignment', 'native_constructor', 'host.assign_private_socket(&base, private)?;', 'if false { host.assign_private_socket(&base, private)?; }'),
    ('rest_lifecycle.rs', 'validation_frozen', 'native_validate_account', 'round_usd(frozen.unwrap_or(0.0)),', 'round_usd(frozen.unwrap_or(0.0)) * 0.0,'),
    ('rest_lifecycle.rs', 'dispose_private', 'native_dispose', 'host.private_dispose()', 'Ok(())'),
    ('rest_lifecycle.rs', 'startup_public_only', 'native_start', 'if public_only {', 'if !public_only {'),
    ('rest_lifecycle.rs', 'tick_closed_context', 'native_tick', '.add_or_update_positions_full(positions)', '.update(StoreUpdate { positions:Some(positions), ..Default::default() },false)'),
    ('rest_lifecycle.rs', 'market_base_units', 'native_fetch_markets', 'min: parse(row.get("minSize"))? * contract,', 'min: parse(row.get("minSize"))?,'),
    ('rest_lifecycle.rs', 'candle_volume', 'native_fetch_ohlcv', 'volume: parse(item(7).as_ref())?,', 'volume: parse(item(5).as_ref())?,'),
    ('api.rs', 'retry_default_budget', 'native_create_api', 'request.timeout_ms = Some(request.timeout_ms.unwrap_or(20_000));', 'request.timeout_ms = Some(request.timeout_ms.unwrap_or(0));'),
    ('api.rs', 'retry_auth_unwind', 'native_create_api', 'for _ in 1..attempt.calls.get() {', 'for _ in 1..1 {'),
]
assert len(controls) == 10
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for index, (file, label, assertion, original, changed) in enumerate(controls[begin:], begin + 1):
    path = root / prefix / file
    saved = path.read_bytes()
    source = saved.decode()
    assert source.count(original) == 1, label + ': target pattern must occur once'
    command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-blofin', '--test', 'rest_lifecycle', '--jobs', '2', assertion, '--', '--exact', '--nocapture']
    try:
        path.write_text(source.replace(original, changed, 1))
        broken = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (out / (label + '-broken.log')).write_text(broken.stdout)
    finally:
        path.write_bytes(saved)
    restored = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (out / (label + '-restored.log')).write_text(restored.stdout)
    caught = broken.returncode == 101 and f'test {assertion} ... FAILED' in broken.stdout and 'panicked at' in broken.stdout
    passed = restored.returncode == 0 and 'test result: ok. 1 passed;' in restored.stdout
    row = dict(sha=sha, label=label, call_site=prefix+file, assertion=assertion, command=command, broken_exit=broken.returncode, restored_exit=restored.returncode, caught=caught, restored=passed)
    results.append(row)
    (out/'results.json').write_text(json.dumps(dict(sha=sha, expected_controls=10, started_at=begin+1, stopped_after=index, results=results), indent=2)+'\n')
    print(json.dumps(row), flush=True)
    assert path.read_bytes() == saved
    if not caught or not passed:
        raise SystemExit('Mutation survived or failed before the named assertion; logs retained: ' + label)
