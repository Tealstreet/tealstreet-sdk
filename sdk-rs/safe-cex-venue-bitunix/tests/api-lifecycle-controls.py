from pathlib import Path
import json
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bitunix'
out = root / 'sdk-rs/target/bitunix-api-lifecycle-evidence'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('api-timeout', 'api.rs', [('20_000.0', '19_000.0')], 'api', 'API native effect create'),
    ('api-signature', 'api.rs', [('format!("{digest}{secret}")', 'format!("{secret}{digest}")')], 'api', 'API native effect headers'),
    ('api-suppression', 'api.rs', [('if suppress {', 'if !suppress {')], 'api', 'API native event count'),
    ('dispose-order', 'exchange_lifecycle.rs', [('base()?;\n    public()?;', 'public()?;\n    base()?;')], 'exchange_lifecycle', 'LIFECYCLE native effect "dispose"'),
    ('shard-cap', 'exchange_lifecycle.rs', [('BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION as usize)', 'BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION as usize - 1)')], 'exchange_lifecycle', 'LIFECYCLE native effect "configurePublicWebsocketShards"'),
    ('listener-once', 'exchange_lifecycle.rs', [
        ('listen_order_book<S, C, R, E>', 'listen_order_book<S: Clone, C: Clone, R, E>'),
        ('listen: impl FnOnce(S, C)', 'mut listen: impl FnMut(S, C)'),
        ('    listen(symbol, callback)', '    listen(symbol.clone(), callback.clone())?;\n    listen(symbol, callback)'),
    ], 'exchange_lifecycle', 'LIFECYCLE native effect "listenOrderBook"'),
    ('income-range-end', 'exchange_lifecycle.rs', [('        start_time,\n        end_time,', '        start_time: end_time,\n        end_time: start_time,')], 'exchange_lifecycle', 'LIFECYCLE native effect "fetchIncomeHistoryRange"'),
]
selected = set(sys.argv[1:])
assert not selected - {c[0] for c in controls}, 'Unknown control'
results = []
for name, file, edits, target, assertion in controls:
    if selected and name not in selected:
        continue
    source = venue / 'src' / file
    original = source.read_text()
    mutant = original
    for before, after in edits:
        assert mutant.count(before) == 1, (name, before)
        mutant = mutant.replace(before, after)
    command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bitunix', '--jobs', '2', '--test', target, '--', '--nocapture']
    try:
        source.write_text(mutant)
        failed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (name + '-mutated.log')).write_text(failed.stdout)
        assert failed.returncode == 101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout, (name, failed.stdout[-2500:])
    finally:
        source.write_text(original)
    passed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / (name + '-restored.log')).write_text(passed.stdout)
    assert passed.returncode == 0, (name, passed.stdout[-2500:])
    results.append({'family': name, 'compiled_exit': failed.returncode, 'assertion': assertion, 'restored_exit': passed.returncode})
    (out / ('selected-controls.json' if selected else 'controls.json')).write_text(json.dumps(results, indent=2) + '\n')
    print(name + ': compiled native failure 101; restored pass 0', flush=True)
