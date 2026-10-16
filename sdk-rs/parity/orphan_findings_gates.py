"""Serial scoped gates; receipts bind command exits to immutable git input."""
from pathlib import Path
import argparse
import json
import os
import re
import shutil
import subprocess
import time

sdk = Path(__file__).resolve().parents[1]
repo = sdk.parent
parser = argparse.ArgumentParser()
parser.add_argument('--out', required=True, type=Path)
parser.add_argument('--checks', action='store_true')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
def guard_inputs():
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip() == sha, 'HEAD changed during gate'
    subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=repo, check=True)
    untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=repo, text=True).splitlines()
    metadata = {'.aimux-brief.md', '.aimux-next.md', '.aimux-priority.md'}
    unexpected = [name for name in untracked if name not in metadata and not (repo / name).resolve().is_relative_to(args.out.resolve())]
    assert not unexpected, 'uncommitted source/fixture inputs: ' + repr(unexpected)
guard_inputs()

cargo = os.environ.get('CARGO') or shutil.which('cargo')
assert cargo
commands = []
if args.checks:
    for crate in ['safe-cex-candle-cache', 'safe-cex-exchange', 'safe-cex-venue-okx', 'safe-cex-store', 'safe-cex', 'safe-cex-venue-bitget', 'safe-cex-parity']:
        commands.append(('check-' + crate, [cargo, 'check', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', crate, '--locked', '--all-targets', '--jobs', '2']))
for name, crate, target, test, ignored in [
    ('assigned-targets', 'safe-cex-parity', 'orphan_findings_targets', None, False),
    ('candle-numeric', 'safe-cex-candle-cache', 'numeric_ingress', None, False),
    ('proxy-numeric', 'safe-cex-exchange', 'numeric_ingress_findings', None, False),
    ('okx-findings', 'safe-cex-venue-okx', 'findings', None, False),
    ('okx-mappers', 'safe-cex-venue-okx', 'mappers', None, True),
    ('okx-rest-operations', 'safe-cex-venue-okx', 'rest_operations', None, True),
    ('instrument-native', 'safe-cex-store', 'instrument_native', None, False),
    ('instrument-regression', 'safe-cex-store', 'instrument_class', None, False),
    ('risk-recovery', 'safe-cex', 'risk_recovery', None, False),
    ('bitget-findings', 'safe-cex-venue-bitget', 'findings_policy', None, False),
    ('bitget-orders', 'safe-cex-venue-bitget', 'order', 'live_order_decisions', True),
    ('bitget-income', 'safe-cex-venue-bitget', 'fill_helpers', 'live_fill_helpers', True),
    ('bitget-bill-page', 'safe-cex-venue-bitget', 'bill_page', 'live_bill_page', True),
]:
    cmd = [cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', crate, '--locked', '--jobs', '2', '--test', target]
    if test:
        cmd.append(test)
    cmd += ['--', '--nocapture']
    if ignored:
        cmd.append('--ignored')
    commands.append((name, cmd))
commands.append(('candle-backend-regression', [cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-candle-cache', '--locked', '--jobs', '2', '--lib', '--', '--nocapture']))
commands.append(('proxy-receiver-regression', [cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-exchange', '--locked', '--jobs', '2', '--lib', 'proxy_token::', '--', '--nocapture']))
commands.append(('base-stats', [cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-exchange', '--locked', '--jobs', '2', '--lib', 'websocket::tests::live_ws_typescript_oracle', '--', '--exact', '--ignored', '--nocapture']))
results = []
for name, cmd in commands:
    print('START', name, flush=True)
    start = time.monotonic()
    run = subprocess.run(cmd, cwd=repo, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log = run.stdout.replace(str(repo), '<worktree>').replace(str(Path.home()), '<home>')
    log = log.replace('/opt/homebrew', '<tool-prefix>')
    (args.out / (name + '.log')).write_text(log)
    count = sum(int(n) for n in re.findall(r'test result: ok\. ([0-9]+) passed;', log)) if 'test' in cmd else None
    accepted = run.returncode == 0 and (count is None or count > 0)
    row = dict(name=name, command=[Path(cmd[0]).name] + cmd[1:], exit=run.returncode, seconds=round(time.monotonic() - start, 2), log=name + '.log', test_count=count, accepted=accepted)
    if run.returncode == 0 and count == 0:
        row['finding'] = 'zero passing tests; command exit 0 is not an accepted gate'
    try:
        guard_inputs()
        row['inputs_unchanged'] = True
    except Exception as error:
        row['inputs_unchanged'] = False
        row['finding'] = repr(error)
        row['accepted'] = False
    results.append(row)
    (args.out / 'gates.json').write_text(json.dumps(dict(sha=sha, gates=results), indent=2) + '\n')
    print('END', name, 'exit=' + str(run.returncode), 'accepted=' + str(row['accepted']), '\n' + '\n'.join(log.splitlines()[-9:]), flush=True)
    if not row['accepted']:
        raise SystemExit(run.returncode or 1)
