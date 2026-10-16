"""Native controls reject compiler errors and failures outside the required assertion."""
from contextlib import contextmanager
from pathlib import Path
import json
import os
import signal
import subprocess

ROOT = Path(__file__).resolve().parents[3]
LOGS = ROOT / '.aimux/bybit-api/controls'
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--jobs', '1']

def target(package, kind, name, test):
    return CARGO + ['-p', package, kind, name, test, '--', '--exact', '--nocapture']

def control(name, file, before, after, command, test, assertion):
    return dict(name=name, file='sdk-rs/' + file, before=before, after=after,
                command=command, test=test, assertion=assertion)

CONTROL_COUNT = 13
CORE = 'safe-cex-exchange'
BYBIT = 'safe-cex-venue-bybit'
controls = [
    control('retry-empty-method-default', BYBIT + '/src/api_request/client.rs',
            'request.method = if request.method.is_empty() {\n                    "get".into()',
            'request.method = if request.method.is_empty() {\n                    String::new()',
            target(BYBIT, '--test', 'api_factory', 'native_retry_failure_config_and_original_read_guard'),
            'native_retry_failure_config_and_original_read_guard', 'factory_retry_config_native_empty_method_reentry_methods'),
    control('configured-region-passthrough', BYBIT + '/src/api_request/client.rs',
            'Some(other) => Some(safe_cex_transport::RestAdapterRegion::Other(other.into())),',
            'Some(_) => return Err(TransportError::Host("region refused".into())),',
            target(BYBIT, '--test', 'api_factory', 'native_proxy_region_construction_matches_source_contract'),
            'native_proxy_region_construction_matches_source_contract', 'factory_region_native_construction 3'),
    control('effective-response-hook-config', CORE + '/src/http.rs',
            'self.host.note_response(response, request);',
            'self.host.note_response(response, &self.request);',
            target(CORE, '--test', 'core_health_http_token', 'http_response_hook_observes_effective_adapter_config'),
            'http_response_hook_observes_effective_adapter_config', 'http_receipt_native_hook_config'),
    control('rejected-response-transform', CORE + '/src/http.rs',
            'if let Some(transform) = &self.response_transform\n                    && let Some(response) = error.response.take()',
            'if error.response.is_none()\n                    && let Some(transform) = &self.response_transform\n                    && let Some(response) = error.response.take()',
            target(CORE, '--test', 'core_health_http_token', 'http_response_transform_precedes_success_and_rejection_hooks'),
            'http_response_transform_precedes_success_and_rejection_hooks', 'http_transform_native_failure_body'),
    control('waiter-immediate-waker-release', CORE + '/src/waiters.rs',
            '.map(|index| entries.remove(index))',
            '.map(|index| { entries.reserve(0); entries[index].clone() })',
            CARGO + ['-p', CORE, '--lib', 'owned_tasks::tests::shared_result_waiter_drop_releases_its_waker_before_task_completion', '--', '--exact', '--nocapture'],
            'owned_tasks::tests::shared_result_waiter_drop_releases_its_waker_before_task_completion', 'shared_native_waiter_drop_releases_waker_immediately'),
    control('retry-response-unwind', BYBIT + '/src/api_factory.rs',
            'for _ in 0..attempt.calls.get() {', 'for _ in 0..1 {',
            target(BYBIT, '--test', 'api_factory', 'native_response_and_clock_reentry'),
            'native_response_and_clock_reentry', 'factory_response_native_auth_failure_unwind'),
    control('original-read-token', 'safe-cex-transport/src/retry.rs',
            'classification.as_ref().is_some_and(|classifier| {',
            'classification.as_ref().is_none_or(|classifier| {',
            target(BYBIT, '--test', 'api_factory', 'native_retry_failure_config_and_original_read_guard'),
            'native_retry_failure_config_and_original_read_guard', 'factory_retry_config_native_attempts private-post-error-method-get'),
    control('shared-signing-clock', BYBIT + '/src/api_request/client.rs',
            'self.clock.get_current_time_ms()', 'self.clock.get_current_time_ms().min(1000.0)',
            target(BYBIT, '--test', 'api_factory', 'native_response_and_clock_reentry'),
            'native_response_and_clock_reentry', 'factory_clock_native_reentry_hmac'),
    control('raw-includes-error-kind', BYBIT + '/src/api_factory.rs',
            'name: "TypeError",', 'name: "Error",',
            target(BYBIT, '--test', 'api_factory', 'native_default_response_transform_and_raw_optional_call'),
            'native_default_response_transform_and_raw_optional_call', 'factory_transform_native_raw_includes_type_error'),
    control('clock-reentry-carried-retry-budget', BYBIT + '/src/api_factory.rs',
            '3_usize.saturating_sub(flags.retries_consumed)', '3_usize',
            target(BYBIT, '--test', 'api_factory', 'native_clock_reentry_retains_consumed_retry_budget'),
            'native_clock_reentry_retains_consumed_retry_budget',
            'factory_clock_budget_native_attempts one-used-before-clock-only-two-left'),
    control('environment-probe-singleflight', BYBIT + '/src/api_factory.rs',
            'let pending = self.env_probe_operation.borrow().clone();',
            'let pending: Option<SharedOperation<ProbeResult>> = None;',
            target(BYBIT, '--test', 'api_factory', 'native_environment_probe_lifecycle'),
            'native_environment_probe_lifecycle', 'factory_probe_native_concurrent_attempts'),
    control('legacy-array-params-own-spread', BYBIT + '/src/api_request.rs',
            'Some(Value::Array(values)) => values\n                .into_iter()\n                .enumerate()\n                .map(|(index, value)| (index.to_string(), value))\n                .collect(),', 'Some(Value::Array(_)) => Map::new(),',
            target(BYBIT, '--test', 'api_factory', 'native_params_spread_matches_both_carriers'),
            'native_params_spread_matches_both_carriers', 'factory_params_native_legacy_keys unsigned-array'),
    control('physical-response-bom', 'safe-cex-transport/src/provider.rs',
            r"text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()", 'text.into_owned()',
            target('safe-cex-transport', '--test', 'response_transform', 'axios_response_transform_observes_current_value_and_physical_decoding'),
            'axios_response_transform_observes_current_value_and_physical_decoding', 'axios_transform_native_value 1'),
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
            child = subprocess.Popen(command, cwd=ROOT, env=env, text=True,
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
    LOGS.mkdir(parents=True, exist_ok=True)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    clean(head)
    env = dict(os.environ, CARGO_INCREMENTAL='0', SAFE_CEX_TYPESCRIPT_PATH=str(ROOT / 'node_modules/typescript'))
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
