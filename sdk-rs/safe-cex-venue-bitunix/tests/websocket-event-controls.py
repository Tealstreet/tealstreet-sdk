from pathlib import Path
import json
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bitunix'
out = root / 'sdk-rs/target/bitunix-websocket-event-evidence'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('public-open', 'websocket_public.rs', 'subscribe_markets("ticker")?', 'subscribe_markets("price")?', 'websocket_events', 'EVENT native effect public.onOpen:markets'),
    ('futures-open', 'websocket_futures.rs', 'schedule("doPingLoop", 15_000.0)', 'schedule("doPingLoop", 14_000.0)', 'websocket_events', 'EVENT native effect futures.onOpen:schedule'),
    ('public-message', 'websocket_public.rs', 'set_count(count()? + 1.0)', 'set_count(count()? + 2.0)', 'websocket_events', 'EVENT native effect public.onMessage:setCount'),
    ('futures-message', 'websocket_futures.rs', 'for index in 0..length {', 'for index in 0..length.saturating_sub(1) {', 'websocket_events', 'EVENT native effect count futures.onMessage'),
    ('ticker-percentage', 'websocket_public.rs', '((last - open) / open) * 100.0', '((last - open) / open) * 10.0', 'websocket_events', 'EVENT native effect public.handleTickerStreamEvent:update'),
    ('price-funding', 'websocket_public.rs', 'parse_source_float(&coerce_field(&data, "fr")?)', 'parse_source_float(&coerce_field(&data, "ft")?)', 'websocket_events', 'EVENT native effect public.handlePriceStreamEvent:field'),
    ('depth-signed-zero', 'websocket_public.rs', 'if bid == 0.0 || bid.is_nan()', 'if bid.is_nan()', 'websocket_events', 'EVENT native effect public.handleDepthBookStreamEvent:update'),
    ('login-delay', 'websocket_private.rs', 'sleep(25.0).await?', 'sleep(24.0).await?', 'websocket_login', 'LOGIN native effect sleep'),
]
selected = set(sys.argv[1:])
assert not selected - {c[0] for c in controls}, 'Unknown control'
results = []
for name, file, before, after, target, assertion in controls:
    if selected and name not in selected:
        continue
    source = venue / 'src' / file
    original = source.read_text()
    assert original.count(before) == 1, (name, before)
    command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bitunix', '--jobs', '2', '--test', target, '--', '--nocapture']
    try:
        source.write_text(original.replace(before, after))
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
