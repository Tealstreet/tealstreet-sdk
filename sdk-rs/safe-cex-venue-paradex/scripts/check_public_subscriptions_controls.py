"""One compiling production mutation for each of eight subscription rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'public_subscriptions', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/public_subscriptions.rs'
original = path.read_text()
controls = [
    ('rpc-id', 'write_id(id + 1.0)?;', 'write_id(id + 2.0)?;', 'rpc_native_counter'),
    ('trade-channel', 'channel("trades.", symbol)', 'channel("wrong.", symbol)', 'trade_native_channel'),
    ('book-sub', 'send(SubscriptionMethod::Subscribe, get_channel(symbol)?)?', 'send(SubscriptionMethod::Unsubscribe, get_channel(symbol)?)?', 'book-sub_native_method'),
    ('book-unsub', 'send(SubscriptionMethod::Unsubscribe, get_channel(symbol)?)?', 'send(SubscriptionMethod::Subscribe, get_channel(symbol)?)?', 'book-unsub_native_method'),
    ('count-sub', 'write_count(symbol, count + 1.0)?;', 'write_count(symbol, count + 2.0)?;', 'count-sub_native_count'),
    ('count-unsub', 'if count <= 1.0 {', 'if count < 1.0 {', 'count-unsub_native_count'),
    ('cleanup-version', 'count_or_zero(read_version(key)?) + 1.0', 'count_or_zero(read_version(key)?) + 2.0', 'cleanup_native_version'),
    ('broadcast-channel', 'channel("bbo.", symbol)', 'channel("wrong.", symbol)', 'broadcast_native_channel'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'public-subscriptions-{label}.log').write_text(output)
    return result.returncode, output


LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, needle, replacement, assertion in controls:
    assert original.count(needle) == 1, (name, 'mutation site changed')
    try:
        path.write_text(original.replace(needle, replacement))
        mutated, output = run(name)
    finally:
        path.write_text(original)
    if mutated == 0:
        report['survivors'].append(name)
    (LOGS / 'public-subscriptions-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert mutated == 101 and 'test result: FAILED' in output, (name, output)
    assert assertion in output, (name, output)
    restored, output = run(name + '-restored')
    assert restored == 0, (name, output)
    report['controls'].append(dict(control=name, assertion=assertion, mutation_exit=mutated, restored_exit=restored))
    (LOGS / 'public-subscriptions-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['controls'][-1]), flush=True)
assert len(report['controls']) == 8
assert path.read_text() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
