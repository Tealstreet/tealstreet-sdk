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
LOGS = ROOT / '.aimux/bybit-local-order/controls'
CARGO = ['cargo', 'test', '--locked', '--jobs', '1']
BYBIT = 'safe-cex-venue-bybit'
BODY = BYBIT + '/src/local_order_reconciliation.rs'
NATIVE = 'local_reconciliation_tests::local_reconciliation_native_source_oracle'
CONTROL_COUNT = 8


def venue_target():
    return CARGO + ['-p', BYBIT, '--test', 'core_snapshot_readers', NATIVE,
                    '--', '--exact', '--include-ignored', '--nocapture']


def control(name, before, after, assertion):
    return dict(name=name, file='sdk-rs/' + BODY, before=before, after=after,
                command=venue_target(), test=NATIVE, assertion=assertion)


controls = [
    control('reconciliation-tolerance',
            'h.numeric(&target)? * 1e-8', 'h.numeric(&target)? * 0.',
            'reconciliation_native_seen_floor tolerance-boundary'),
    control('reconciliation-strict-grace-boundary',
            '< RECENTLY_PLACED_ORDER_GRACE_MS', '<= RECENTLY_PLACED_ORDER_GRACE_MS',
            'reconciliation_native_orders_floor below-tolerance'),
    control('reconciliation-disposed-after-fills',
            'let fills = h.wait(fills).await?;\n            if h.truthy(&h.get(&owner, "isDisposed")?) {',
            'let fills = h.wait(fills).await?;\n            if { let observed = h.get(&owner, "isDisposed")?; drop(observed); false } {',
            'reconciliation_native_seen_floor disposed-after-fills'),
    control('reconciliation-logger-error-retention',
            'h.apply(log, vec![text, warning])?;',
            'drop(h.apply(log, vec![text, warning]));',
            'reconciliation_native_rejection_floor'),
    control('reconciliation-order-predicate',
            'h.boolean(h.equal(&h.get(row, "id")?, id))',
            'h.boolean(!h.equal(&h.get(row, "id")?, id))',
            'reconciliation_native_predicate_false_then_true'),
]
WRITE_NATIVE = 'primary_local_order_write_tests::primary_local_order_write_native_source_oracle'
WRITE_COMMAND = CARGO + ['-p', BYBIT, '--test', 'spot_order_writes', WRITE_NATIVE,
                         '--', '--exact', '--include-ignored', '--nocapture']
controls.extend([
    dict(name='record-opts-bare-parse-float-receiver',
         file='sdk-rs/' + BYBIT + '/src/spot_order_writes.rs',
         before='let f = h.get(&h.global(), "parseFloat")?;',
         after='let f = h.select(h.global(), "parseFloat")?;',
         command=WRITE_COMMAND, test=WRITE_NATIVE,
         assertion='local_write_native_parse_receiver'),
    dict(name='record-opts-conditional-clearing',
         file='sdk-rs/' + BYBIT + '/src/spot_order_writes.rs',
         before='let reduce = if is(h, &p(h, &opts, "type")?, "OrderType", "Conditional")? {\n        h.boolean(false)',
         after='let reduce = if is(h, &p(h, &opts, "type")?, "OrderType", "Conditional")? {\n        h.boolean(true)',
         command=WRITE_COMMAND, test=WRITE_NATIVE,
         assertion='local_write_native_reduce_only conditional-clear'),
    dict(name='local-update-original-filled',
         file='sdk-rs/' + BYBIT + '/src/primary_local_order_writes.rs',
         before='h.get(&order, "filled")?', after='h.get(&update, "filled")?',
         command=WRITE_COMMAND, test=WRITE_NATIVE,
         assertion='local_write_native_original_filled_remaining amount'),
])
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
