#!/usr/bin/env python3
"""Compiled production faults through the assembled BingX audit probes; exact restore."""
import argparse
import pathlib
import subprocess

crate = pathlib.Path(__file__).resolve().parents[1]
workspace = crate.parent
parser = argparse.ArgumentParser()
parser.add_argument('--only', action='append')
args = parser.parse_args()

def replace_once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new, 1)

def lazy_boundary(text, method):
    start = text.index('fn ' + method + '(')
    body = text.index('{', start)
    depth, end = 1, body + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    call = text.index('safe_cex_exchange::eager::start_eager(', body, end)
    expression = text[call:end-1].strip()
    return text[:call] + 'Box::pin(async move { ' + expression + '.await })\n    ' + text[end-1:]

controls = {
    'native-family-prefix': ('src/binding/exchange.rs', lambda s: lazy_boundary(s, 'change_margin_type'), 'real_native_family_margin_error_runs_before_poll', '--test=divergence_timing'),
    'native-family-continuation': ('src/binding/exchange.rs', lambda s: lazy_boundary(s, 'change_position_mode'), 'real_native_family_dropped_mode_waiter_keeps_held_continuation', '--test=divergence_timing'),
    'ohlcv': ('src/binding/exchange.rs', lambda s: lazy_boundary(s, 'fetch_ohlcv'), 'real_ohlcv_missing_market_emits_before_future_is_polled', '--test=divergence_timing'),
    'fills': ('src/binding/exchange.rs', lambda s: lazy_boundary(s, 'fetch_historical_fills'), 'real_historical_fills_logs_params_before_future_is_polled', '--test=divergence_timing'),
    'tick': ('src/read_history.rs', lambda s: lazy_boundary(s, 'tick'), 'real_tick_starts_selected_read_before_return', '--lib'),
    'timer-prefix': ('src/binding/hosts.rs', lambda s: replace_once(s, '        let future = safe_cex_exchange::eager::start_eager(&runtime, future);', ''), 'real_timer_recovery_closes_stale_socket_before_next_runtime_job', '--lib'),
    'rebuild': ('src/listen_key.rs', lambda s: replace_once(s, 'self.host.close_socket(socket)?;', 'let _ = socket;'), 'real_message_rebuild_detaches_socket_in_call_prefix', '--lib'),
    'open': ('src/listen_key.rs', lambda s: replace_once(s, 'self.schedule_listen_key_refresh(None)?;', 'self.schedule_listen_key_refresh(Some(1.0))?;'), 'real_open_refresh_and_disposal_cancel_live_host_timer', '--lib'),
    'recovery': ('src/listen_key.rs', lambda s: replace_once(s, 'self.timer(30.0 * 1000.0, true)', 'self.timer(30.0 * 1000.0 + 1.0, true)'), 'real_recovery_timer_replaces_refresh_and_rebuilds_at_30_seconds', '--lib'),
    'dispose': ('src/listen_key.rs', lambda s: replace_once(s, '''        *self.listen_key.borrow_mut() = None;
        self.host.base_dispose()''', '''        self.host.base_dispose()'''), 'real_open_refresh_and_disposal_cancel_live_host_timer', '--lib'),
    'drop': ('src/binding/exchange.rs', lambda s: replace_once(s, 'let _ = self.dispose();', 'let _ = Ok::<(), ExchangeError>(());'), 'final_handle_drop_tears_down_everything', '--lib'),
}

def run(selector, target):
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-bingx', target, selector]
    return subprocess.run(command, cwd=workspace, text=True, capture_output=True)

for name in args.only or controls:
    relative, fault, selector, target = controls[name]
    path = crate / relative
    original = path.read_bytes()
    baseline = run(selector, target)
    assert baseline.returncode == 0 and 'test result: ok.' in baseline.stdout, baseline.stdout + baseline.stderr
    try:
        path.write_text(fault(original.decode()))
        result = run(selector, target)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and ' ... FAILED' in output and 'test result: FAILED.' in output, output
        assert 'could not compile' not in output, output
        print(f'{name}: compiled named RED', flush=True)
    finally:
        path.write_bytes(original)
        assert path.read_bytes() == original
    restored = run(selector, target)
    assert restored.returncode == 0 and 'test result: ok.' in restored.stdout, restored.stdout + restored.stderr
    print(f'{name}: exact restore GREEN', flush=True)
