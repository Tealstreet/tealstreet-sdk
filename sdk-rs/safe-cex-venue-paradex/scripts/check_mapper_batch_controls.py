"""One discriminating native assertion for each newly mapped row."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
LOGS = ROOT / '.aimux/paradex'
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'mapper_batch', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=
           '/Users/sam/cs/tealstreet-next/node_modules/typescript')
CONTROLS = [
    ('order-amount', 'order_mapper.rs', '        amount,',
     '        amount: remaining,', 'order_native_amount'),
    ('time-in-force', 'order_maps.rs', '("FOK", "IOC")',
     '("FOK", "GTC")', 'tif_native_value'),
    ('order-type', 'order_maps.rs', 'OrderType::Conditional => "STOP_LIMIT"',
     'OrderType::Conditional => "LIMIT"', 'type_native_value'),
    ('channel', 'public_ws_helpers.rs', '.snapshot@15@100ms',
     '.snapshot@15@200ms', 'channel_native_value'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'mapper-batch-{label}.log').write_text(output)
    return result.returncode, output


head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, file, needle, replacement, assertion in CONTROLS:
    source = CRATE / 'src' / file
    original = source.read_text()
    assert original.count(needle) == 1, (name, needle)
    try:
        source.write_text(original.replace(needle, replacement))
        mutated, output = run(name)
    finally:
        source.write_text(original)
    assert mutated == 101 and 'test result: FAILED' in output, (name, output)
    assert assertion in output, (name, output)
    restored, output = run(name + '-restored')
    assert restored == 0, (name, output)
    row = dict(control=name, assertion=assertion, mutation_exit=mutated, restored_exit=restored)
    report['controls'].append(row)
    (LOGS / 'mapper-batch-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 4
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
print(json.dumps(dict(head=head, controls_attempted=4, survivors=[])), flush=True)
