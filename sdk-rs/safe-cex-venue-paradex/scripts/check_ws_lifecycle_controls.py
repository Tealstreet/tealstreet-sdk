"""One production control for each of ten newly ported websocket rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'ws_lifecycle', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = []
for venue in ['public', 'private']:
    file = f'src/ws_{venue}_lifecycle.rs'
    controls.extend([
        (venue + '-connect', file, 'add_listener("open")?;', 'add_listener("wrong")?;', venue + '_connect_native_event'),
        (venue + '-ping', file, 'schedule(callback, 30_000)', 'schedule(callback, 30_001)', venue + '_ping_native_delay'),
        (venue + '-dispose', file, '    base_dispose()\n}', '    let _retained_base = base_dispose;\n    Ok(())\n}', venue + '_dispose_native_base'),
    ])
controls.extend([
    ('public-open', 'src/ws_public_lifecycle.rs', '        clear_sent()?;\n        clear_pending()?;', '        clear_pending()?;\n        clear_sent()?;', 'public_open_native_before_trace'),
    ('interest', 'src/ws_public_lifecycle.rs', 'read_market_length()? > 0.0', 'read_market_length()? >= 0.0', 'interest_native_value'),
    ('throttle', 'src/ws_private_lifecycle.rs', 'schedule(read_fetch_callback()?, 1_000)', 'schedule(read_fetch_callback()?, 999)', 'throttle_native_delay'),
    ('channels', 'src/ws_private_lifecycle.rs', '["orders.ALL", "positions", "balance_events"]', '["orders.ALL", "positions", "balances"]', 'channels_native_channel'),
])


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'ws-lifecycle-{label}.log').write_text(output)
    return result.returncode, output


LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, file, needle, replacement, assertion in controls:
    path = CRATE / file
    original = path.read_text()
    assert original.count(needle) == 1, (name, 'mutation site changed')
    try:
        path.write_text(original.replace(needle, replacement))
        mutated, output = run(name)
    finally:
        path.write_text(original)
    if mutated == 0:
        report['survivors'].append(name)
    (LOGS / 'ws-lifecycle-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert mutated == 101 and 'test result: FAILED' in output, (name, output)
    assert assertion in output, (name, output)
    restored, output = run(name + '-restored')
    assert restored == 0, (name, output)
    report['controls'].append(dict(control=name, assertion=assertion, mutation_exit=mutated, restored_exit=restored))
    (LOGS / 'ws-lifecycle-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['controls'][-1]), flush=True)
assert len(report['controls']) == 10
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
