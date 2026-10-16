"""One compiled production mutation for each of eight new public ingress rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'public_ingress', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/public_ingress.rs'
original = path.read_text()
controls = [
    ('constructor-network', 'write(lookup(map, testnet(parent)?)?)', 'write(lookup(map, !testnet(parent)?)?)', 'constructor_native_url'),
    ('message-stats', 'h.literal("order_book")', 'h.literal("trades")', 'message_native_stats'),
    ('retry-delay', 'schedule(1_000.0 * (attempt + 1.0), attempt + 1.0)?', 'schedule(2_000.0 * (attempt + 1.0), attempt + 1.0)?', 'retry_native_delay'),
    ('bbo-bid', 'positive(&mut fields, "bid", bid);', 'positive(&mut fields, "ask", bid);', 'bbo_native_fields'),
    ('summary-percentage', 'value * 100.0', 'value * 10.0', 'summary_native_fields'),
    ('trades-mark', 'vec![("last", price), ("mark", price)]', 'vec![("last", price), ("mark", price + 1.0)]', 'trades_native_fields'),
    ('candle-volume', 'h.add(&h.get(&previous, "volume")?, size)?', 'h.add(&h.get(&previous, "volume")?, size + 1.0)?', 'candle_native_volume'),
    ('recovery-force', 'subscribe(true)', 'subscribe(false)', 'recovery_native_force'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'public-ingress-{label}.log').write_text(output)
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
    (LOGS / 'public-ingress-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'public-ingress-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
