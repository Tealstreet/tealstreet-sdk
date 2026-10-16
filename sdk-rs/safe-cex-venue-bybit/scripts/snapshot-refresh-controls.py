"""Reject compiler failures and failures outside each required native assertion."""
from contextlib import contextmanager
from pathlib import Path
import json
import os
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
LOGS = ROOT / '.aimux/bybit-snapshot/controls'
CARGO = ['cargo', 'test', '--locked', '--jobs', '1']
CORE = 'safe-cex-exchange'
BYBIT = 'safe-cex-venue-bybit'
BODY = BYBIT + '/src/private_snapshot_refresh.rs'
NATIVE = 'private_snapshot_refresh_tests::private_snapshot_refresh_native_source_oracle'
CONTROL_COUNT = 10


def venue_target():
    return CARGO + ['-p', BYBIT, '--test', 'core_snapshot_readers', NATIVE,
                    '--', '--exact', '--include-ignored', '--nocapture']


def core_target(name):
    return CARGO + ['-p', CORE, '--lib', 'owned_tasks::tests::' + name,
                    '--', '--exact', '--nocapture']


def control(name, file, before, after, command, test, assertion):
    return dict(name=name, file='sdk-rs/' + file, before=before, after=after,
                command=command, test=test, assertion=assertion)


controls = [
    control('global-timeout-retained-registry', CORE + '/src/owned_tasks.rs',
            'retain_owner: true,', 'retain_owner: false,',
            core_target('detached_timeout_survives_an_unrelated_disposal_and_handle_drop'),
            'owned_tasks::tests::detached_timeout_survives_an_unrelated_disposal_and_handle_drop',
            'detached_timer_native_capture_pending'),
    control('global-timeout-explicit-cancellation', CORE + '/src/owned_tasks.rs',
            'owner.finish(self.id, true);', 'drop(owner);',
            core_target('cancelling_detached_timeout_releases_its_sleep_and_callback'),
            'owned_tasks::tests::cancelling_detached_timeout_releases_its_sleep_and_callback',
            'detached_timer_native_cancel_sleep'),
    control('snapshot-trailing-delay', BODY,
            'PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS: f64 = 250.;',
            'PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS: f64 = 251.;',
            venue_target(), NATIVE, 'snapshot_native_independent_invocation_times single'),
    control('snapshot-max-wait', BODY,
            'PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS: f64 = 1200.;',
            'PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS: f64 = 2000.;',
            venue_target(), NATIVE, 'snapshot_native_independent_invocation_times max-wait'),
    control('snapshot-disposed-guard', BODY,
            'h.truthy(&h.get(&owner, "isDisposed")?)',
            'h.truthy(&{ let observed = h.get(&owner, "isDisposed")?; drop(observed); h.boolean(false) })',
            venue_target(), NATIVE, 'snapshot_native_independent_invocation_times disposed'),
    control('snapshot-original-error-reporter', BODY,
            'h.apply(selected, vec![h.error_value(error)])?;',
            'drop((selected, h.error_value(error)));',
            venue_target(), NATIVE, 'snapshot_native_report_count rejected'),
    control('snapshot-cancel-retains-result', BODY,
            'state.last_invoke = 0.;',
            'state.last_invoke = 0.;\n            state.result = self.host.undefined();',
            venue_target(), NATIVE, 'snapshot_native_cancel_retains_result'),
    control('snapshot-post-disposal-reporting', BODY,
            'if let Err(error) = refreshed {',
            'if let Err(error) = refreshed && !h.truthy(&h.get(&owner, "isDisposed")?) {',
            venue_target(), NATIVE, 'snapshot_native_post_disposal_report_floor'),
    control('snapshot-selected-helper-receiver', BODY,
            'h.select(owner.clone(), "fetchAndSetPrivateSnapshotHelper")?;',
            'h.select(h.undefined(), "fetchAndSetPrivateSnapshotHelper")?;',
            venue_target(), NATIVE, 'snapshot_native_selected_receiver_identity'),
    control('snapshot-reporter-rejection-retained', BODY,
            'h.apply(selected, vec![h.error_value(error)])?;',
            'drop(h.apply(selected, vec![h.error_value(error)]));',
            venue_target(), NATIVE, 'snapshot_native_rejected_result_count reporter-rejection'),
]
assert len(controls) == CONTROL_COUNT

def clean(head):
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
    subprocess.run(['git', 'diff', '--exit-code'], cwd=ROOT, check=True, capture_output=True)
    subprocess.run(['git', 'diff', '--cached', '--exit-code'], cwd=ROOT, check=True, capture_output=True)

@contextmanager
def defer_termination():
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGINT, signal.SIGTERM})
    try:
        yield
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)

def terminate_child(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        child.wait()
    try:
        return child.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            child.wait()
        return child.communicate()

class ControlInterrupted(RuntimeError):
    pass

spawning_child = False
pending_signal = None

def interrupted(signum, _frame):
    global pending_signal
    if spawning_child:
        pending_signal = signum
        return
    raise ControlInterrupted(f'control interrupted by signal {signum}')

def execute(command, name, stage):
    global spawning_child
    child = None
    try:
        spawning_child = True
        try:
            child = subprocess.Popen(command, cwd=SDK, env=env, text=True,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     start_new_session=True)
        finally:
            spawning_child = False
        if pending_signal is not None:
            interrupted(pending_signal, None)
        stdout, stderr = child.communicate()
    except BaseException:
        with defer_termination():
            if child is not None:
                stdout, stderr = terminate_child(child)
                (LOGS / f'{name}-{stage}.log').write_text(stdout + stderr)
        raise
    output = stdout + stderr
    (LOGS / f'{name}-{stage}.log').write_text(output)
    return child.returncode, output

def save():
    with defer_termination():
        (LOGS / 'report.json').write_text(json.dumps(report, indent=2) + chr(10))

if __name__ == '__main__':
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    clean(head)
    LOGS = LOGS / head / str(time.time_ns())
    LOGS.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1', SAFE_CEX_TYPESCRIPT_PATH=str(ROOT / 'node_modules/typescript'))
    report = dict(measured_sha=head, accepted=False, declared_controls=CONTROL_COUNT, attempts=[], completed_controls=[])
    save()
    positive_commands = set()
    try:
        for item in controls:
            clean(head)
            path = ROOT / item['file']
            original = path.read_bytes()
            source = original.decode()
            assert source.count(item['before']) == 1, f"unique mutation site required: {item['name']}"
            print('START', item['name'], head, flush=True)
            key = tuple(item['command'])
            if key not in positive_commands:
                code, output = execute(item['command'], item['name'], 'positive')
                assert code == 0 and f"test {item['test']} ... ok" in output, f"native positive target failed: {item['name']} exit {code}"
                positive_commands.add(key)
            attempt = dict(name=item['name'], mutation_exit=None, restore_exit=None, required_assertion=item['assertion'])
            report['attempts'].append(attempt)
            save()
            try:
                path.write_text(source.replace(item['before'], item['after'], 1))
                code, output = execute(item['command'], item['name'], 'mutation')
                attempt['mutation_exit'] = code
            finally:
                with defer_termination():
                    path.write_bytes(original)
                    attempt['source_restored'] = path.read_bytes() == original
                    save()
            save()
            assert code == 101 and 'test result: FAILED' in output and 'running 1 test' in output and item['assertion'] in output and 'could not compile' not in output, f"REJECTED mutation outcome: {item['name']} exit {code}; compiler errors and unrelated assertions are not controls"
            clean(head)
            code, output = execute(item['command'], item['name'], 'restore')
            attempt['restore_exit'] = code
            save()
            assert code == 0 and f"test {item['test']} ... ok" in output, f"REJECTED restore outcome: {item['name']} exit {code}"
            clean(head)
            report['completed_controls'].append(item['name'])
            save()
            print('CONTROL', item['name'], '101 then 0', flush=True)
        assert len(report['completed_controls']) == CONTROL_COUNT
        report['accepted'] = True
        save()
        print('COMPLETE', head, f'{CONTROL_COUNT} compiled controls; no survivors among the {CONTROL_COUNT} attempted', flush=True)
    except BaseException as error:
        report['accepted'] = False
        report['rejected_reason'] = str(error)
        report['withdrawn_completed_controls'] = report['completed_controls']
        report['completed_controls'] = []
        save()
        raise
