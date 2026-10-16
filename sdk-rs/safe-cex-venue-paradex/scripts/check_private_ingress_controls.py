"""One compiled production mutation for each of eight new private ingress rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'private_ingress', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/private_ingress.rs'
original = path.read_text()
controls = [
    ('constructor-binding', 'channel: "orders.ALL",', 'channel: "orders.WRONG",', 'constructor_native_bindings'),
    ('status-filled', 'Some(OrderStatus::Closed)', 'Some(OrderStatus::Open)', 'status_native_value'),
    ('cancel-reason', '"Insufficient margin/balance"', '"Wrong margin/balance"', 'reason_native_value'),
    ('position-throttle', 'if elapsed >= read_period()? {', 'if elapsed > read_period()? {', 'throttle_native_before_state'),
    ('message-auth', 'h.write_authenticated(true)?;', 'h.write_authenticated(false)?;', 'message_native_authenticated'),
    ('order-filled', 'filled: h.parse_float(&h.get(&order, "size")?)?\n                - h.parse_float(&h.get(&order, "remaining_size")?)?,', 'filled: h.parse_float(&h.get(&order, "size")?)?\n                + h.parse_float(&h.get(&order, "remaining_size")?)?,', 'order_native_filled'),
    ('position-notional', 'notional: contracts * entry,', 'notional: contracts + entry,', 'position_native_notional'),
    ('balance-upnl', 'upnl: 0.0,', 'upnl: 1.0,', 'balance_native_upnl'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'private-ingress-{label}.log').write_text(output)
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
    record = dict(control=name, assertion=assertion, mutation_exit=mutated,
                  compiled_runtime_failure=mutated == 101 and 'test result: FAILED' in output)
    if mutated == 0:
        report['survivors'].append(name)
    report['controls'].append(record)
    (LOGS / 'private-ingress-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'private-ingress-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
