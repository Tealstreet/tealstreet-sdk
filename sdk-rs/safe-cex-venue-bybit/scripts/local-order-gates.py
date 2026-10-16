"""Run serial native and CI gates against one clean committed tree."""
from pathlib import Path
import json
import os
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
CARGO = ['cargo', 'test', '--locked', '--jobs', '1']
GATES = [
    ('workspace-fmt', ['cargo', 'fmt', '--check'], SDK, None),
    ('diff-check', ['git', 'diff', '--check'], ROOT, None),
    ('local-reconciliation', CARGO + ['-p', 'safe-cex-venue-bybit', '--test', 'core_snapshot_readers', 'local_reconciliation_tests::local_reconciliation_native_source_oracle', '--', '--exact', '--include-ignored'], SDK, 1),
    ('primary-local-order-writes', CARGO + ['-p', 'safe-cex-venue-bybit', '--test', 'spot_order_writes', 'primary_local_order_write_tests::primary_local_order_write_native_source_oracle', '--', '--exact', '--include-ignored'], SDK, 1),
    ('existing-spot-order-writes', CARGO + ['-p', 'safe-cex-venue-bybit', '--test', 'spot_order_writes', 'spot_', '--', '--include-ignored'], SDK, 2),
    ('workspace-build', ['cargo', 'build', '--workspace', '--locked'], SDK, None),
]


def main():
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    folder = ROOT / '.aimux/bybit-local-order/gates' / head / str(time.time_ns())
    folder.mkdir(parents=True, exist_ok=False)
    report = dict(measured_sha=head, accepted=False, declared_gates=len(GATES), gates=[])
    env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1',
               SAFE_CEX_TYPESCRIPT_PATH=str(ROOT / 'node_modules/typescript'))

    def save():
        (folder / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    def clean():
        assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
        for command in [['git', 'diff', '--exit-code'], ['git', 'diff', '--cached', '--exit-code']]:
            subprocess.run(command, cwd=ROOT, check=True, capture_output=True)

    save()
    try:
        assert len(GATES) == report['declared_gates']
        for name, command, cwd, count in GATES:
            clean()
            print('START', name, head, flush=True)
            started = time.monotonic()
            log = folder / (name + '.log')
            with log.open('w') as output:
                result = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT)
            floor = count is None or f'test result: ok. {count} passed; 0 failed; 0 ignored;' in log.read_text()
            report['gates'].append(dict(name=name, exit=result.returncode, command=command,
                                        expected_tests=count, test_floor_observed=floor,
                                        elapsed_seconds=round(time.monotonic() - started, 2)))
            save()
            print('EXIT', name, result.returncode, flush=True)
            assert result.returncode == 0 and floor, f'rejected gate {name}: exit {result.returncode}, floor {floor}'
            clean()
        report['accepted'] = True
        save()
        print('COMPLETE', head, str(folder.relative_to(ROOT)), flush=True)
    except BaseException as error:
        report['accepted'] = False
        report['rejected_reason'] = str(error)
        save()
        raise


if __name__ == '__main__':
    main()
