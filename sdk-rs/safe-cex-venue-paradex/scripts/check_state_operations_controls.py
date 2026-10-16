"""One discriminating production mutation for each newly ported state operation."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'state_operations', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/state_operations.rs'
original = path.read_text()
controls = [
    ('public-mode', 'return Ok(true);', 'return Ok(false);', 'public_native_value'),
    ('reset-order', '    write_init(None)?;\n    write_account(None)?;',
     '    write_account(None)?;\n    write_init(None)?;', 'reset_native_trace'),
    ('history-scope', 'fold_closed_position_records(opts, true)?',
     'fold_closed_position_records(opts, false)?', 'history_native_symbol_scoped'),
    ('sdk-cache', 'if cache.borrow().is_none() {', 'if true {', 'cache_native_before_settled'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'state-operations-{label}.log').write_text(output)
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
    (LOGS / 'state-operations-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert mutated == 101 and 'test result: FAILED' in output, (name, output)
    assert assertion in output, (name, output)
    restored, output = run(name + '-restored')
    assert restored == 0, (name, output)
    row = dict(control=name, assertion=assertion, mutation_exit=mutated, restored_exit=restored)
    report['controls'].append(row)
    (LOGS / 'state-operations-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 4
assert path.read_text() == original
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
