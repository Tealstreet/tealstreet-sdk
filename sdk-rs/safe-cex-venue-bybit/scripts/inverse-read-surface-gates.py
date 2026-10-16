"""Seven serial gates pinned to one committed inverse read tree."""
from pathlib import Path
import json
import os
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
LOGS = ROOT / '.aimux/bybit-inverse-reads'
ENV = dict(os.environ, CARGO_INCREMENTAL='1', SAFE_CEX_TYPESCRIPT_PATH=str(ROOT / 'node_modules/typescript'))
YARN_CLI = Path(os.environ['YARN_CLI'])
assert YARN_CLI.is_file(), 'YARN_CLI must name the installed Yarn 1.22.21 JavaScript entry point'
LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
base = subprocess.check_output(['git', 'rev-parse', sys.argv[1]], cwd=ROOT, text=True).strip()
subprocess.run(['git', 'diff', '--exit-code'], cwd=ROOT, check=True, capture_output=True)
commands = [
    ('scoped-cargo-check', ['cargo', 'check', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-venue-bybit', '--all-targets', '--jobs', '1']),
    ('native-controls', ['python3.14', 'sdk-rs/safe-cex-venue-bybit/scripts/inverse-read-surface-controls.py']),
    ('extract-check', ['node', str(YARN_CLI), 'parity:extract:check']),
    ('unit-structure', ['python3.14', 'sdk-rs/safe-cex-venue-bybit/scripts/inverse-read-surface-structure.py']),
    ('unit-notes', ['python3.14', 'sdk-rs/safe-cex-venue-bybit/scripts/inverse-read-surface-notes.py']),
    ('scoped-rustfmt', ['rustfmt', '--edition', '2024', '--check', 'sdk-rs/safe-cex-venue-bybit/src/inverse_read_surface.rs', 'sdk-rs/safe-cex-venue-bybit/tests/inverse_read_surface.rs']),
    ('diff-check', ['git', 'diff', '--check']),
]
report = dict(head=head, base=base, gates=[])
for name, command in commands:
    print('START', name, head, flush=True)
    result = subprocess.run(command, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / ('gate-' + name + '.log')).write_text(output)
    report['gates'].append(dict(name=name, command=command, exit=result.returncode, measured_sha=head))
    (LOGS / 'gates.json').write_text(json.dumps(report, indent=2) + '\n')
    print('EXIT', name, result.returncode, flush=True)
    if result.returncode:
        print(output[-14000:])
        raise SystemExit(1)
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
    subprocess.run(['git', 'diff', '--exit-code'], cwd=ROOT, check=True, capture_output=True)
assert len(report['gates']) == 7
print('COMPLETE', head, flush=True)
