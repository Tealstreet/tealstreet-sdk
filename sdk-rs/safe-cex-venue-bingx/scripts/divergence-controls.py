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

controls = {
    'rebuild': ('src/listen_key.rs', lambda s: replace_once(s, 'self.host.close_socket(socket)?;', 'let _ = socket;'), 'real_message_rebuild_detaches_socket_in_call_prefix', '--lib'),
    'open': ('src/listen_key.rs', lambda s: replace_once(s, 'self.schedule_listen_key_refresh(None)?;', 'self.schedule_listen_key_refresh(Some(1.0))?;'), 'real_open_refresh_and_disposal_cancel_live_host_timer', '--lib'),
    'recovery': ('src/listen_key.rs', lambda s: replace_once(s, 'self.timer(30.0 * 1000.0, true)', 'self.timer(30.0 * 1000.0 + 1.0, true)'), 'real_recovery_timer_replaces_refresh_and_rebuilds_at_30_seconds', '--lib'),
    'dispose': ('src/listen_key.rs', lambda s: replace_once(s, '''        *self.listen_key.borrow_mut() = None;
        self.host.base_dispose()''', '''        self.host.base_dispose()'''), 'real_open_refresh_and_disposal_cancel_live_host_timer', '--lib'),
    'drop': ('src/binding/exchange.rs', lambda s: replace_once(s, 'if let Err(error) = self.dispose() {', 'if let Err(error) = Ok::<(), ExchangeError>(()) {'), 'final_handle_drop_tears_down_everything', '--lib'),
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
