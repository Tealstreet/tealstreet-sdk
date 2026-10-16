import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/account_actions.rs'
out = root / 'sdk-rs/target/weex-account-actions-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'account_actions']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('normal-cancellation', 'cancel_orders', 'normal\n            .chunks(10)', 'normal\n            .chunks(9)', 'native_requests::cancel_batches'),
    ('parallel-rejection', 'cancel_symbol_orders', '''    let aggregate = match (runtime.promise_all)(host, first, second) {
        Ok(future) => future,
        Err(error) => return ready(Err(error)),
    };''', '''    let aggregate: ActionFuture<'a, (), E> = Box::pin(async move {
        first.await?;
        second.await
    });''', 'native_error_message::symbol_dual_rejection'),
    ('isolated-leverage', 'set_leverage', 'side == Some(PositionSide::Short)', 'side == Some(PositionSide::Long)', 'native_requests::leverage_isolated_long'),
    ('metadata-refetch', 'update_symbol_metadata', '(runtime.config)(host, target, true)', '(runtime.config)(host, target, false)', 'native_config_calls::leverage_cross'),
    ('close-positions-update', 'nuke_positions', '(runtime.update_positions)(host, positions)', '{ drop(positions); Ok(()) }', 'native_positions::close_positions'),
]
results = []
for name, function, before, after, assertion in controls:
    original = source.read_bytes()
    text = original.decode()
    start = text.index('pub fn ' + function + '<')
    end = text.find('\npub fn ', start + 1)
    if end < 0:
        end = len(text)
    segment = text[start:end]
    assert segment.count(before) == 1, (name, segment.count(before))
    mutated = text[:start] + segment.replace(before, after) + text[end:]
    try:
        source.write_text(mutated)
        broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (name + '-broken.log')).write_text(broken.stdout)
        assert broken.returncode == 101, (name, broken.returncode)
        assert assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout, (name, broken.stdout)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / (name + '-restored.log')).write_text(restored.stdout)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    row = {'control': name, 'function': function, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode}
    results.append(row)
    print(json.dumps(row), flush=True)
(out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
