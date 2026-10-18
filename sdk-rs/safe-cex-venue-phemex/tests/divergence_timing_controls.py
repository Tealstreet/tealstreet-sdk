#!/usr/bin/env python3
"""Independent production breaks: compiled named RED, exact bytes restored GREEN."""
import os
from pathlib import Path
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
out = workspace / 'target/phemex-divergence-controls'
out.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
if 'SAFE_CEX_TYPESCRIPT_PATH' not in env:
    common = subprocess.check_output(['git', 'rev-parse', '--path-format=absolute', '--git-common-dir'], cwd=workspace, text=True).strip()
    env['SAFE_CEX_TYPESCRIPT_PATH'] = str(Path(common).parent / 'node_modules/typescript')

def replace_once(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new, 1)

def deferred(text, unit):
    start = text.index('    fn invoke_unit(' if unit else '    fn invoke<T:')
    end = text.index('    fn listen<' if unit else '    fn invoke_unit(', start)
    body = text[start:end]
    body = replace_once(body, '        let result = self.rt.this.call_method(member, args);\n', '        let this = self.rt.this.clone();\n')
    body = replace_once(body, '        Box::pin(async move {\n', '        Box::pin(async move {\n            let result = this.call_method(member, args);\n')
    return text[:start] + body + text[end:]

controls = [
    ('typed-lazy', 'binding/exchange.rs', lambda t: deferred(t, False), 'ticker_sdk_starts_http_before_poll', 'PHEMEX eager typed member'),
    ('unit-lazy', 'binding/exchange.rs', lambda t: deferred(t, True), 'unit_sdk_starts_source_parallel_reads_before_poll', 'PHEMEX eager unit member'),
    ('ticker-abandonment', 'binding/exchange.rs', lambda t: deferred(t, False), 'discarded_ticker_waiter_keeps_error_emission_after_http', 'PHEMEX discarded SDK waiter'),
    ('sync-delayed-prefix', 'binding/sockets.rs', lambda t: replace_once(t,
        '                        let started = private::begin_sync(&mut h);\n                        let rt = h.rt.clone();\n                        return Ok(rt.spawn(Box::pin(async move {\n',
        '                        let rt = h.rt.clone();\n                        return Ok(rt.spawn(Box::pin(async move {\n                            std::future::pending::<()>().await;\n                            let started = private::begin_sync(&mut h);\n'),
        'reconciliation_clears_queued_symbols_even_when_disposed', 'PHEMEX source pre-try symbol clearing'),
    ('sync-visible-error', 'binding/sockets.rs', lambda t: replace_once(t,
        'let _ = private::finish_sync(&mut h, started).await;',
        'private::finish_sync(&mut h, started).await?;'),
        'reconciliation_silently_fulfills_store_failure_on_real_binding', 'PHEMEX source silent catch'),
    ('sync-lost-continuation', 'binding/sockets.rs', lambda t: replace_once(t,
        'let _ = private::finish_sync(&mut h, started).await;',
        'drop(private::finish_sync(&mut h, started));'),
        'reconciliation_retains_original_receiver_and_pending_store_commit', 'PHEMEX discarded reconciliation Promise'),
    ('sync-after-dispose', 'private_session.rs', lambda t: replace_once(t,
        '    let (balance, positions) = host.parts(account)?;\n    if host.disposed()? {',
        '    let (balance, positions) = host.parts(account)?;\n    if host.disposed()? && false {'),
        'reconciliation_checks_disposal_after_pending_account_read', 'PHEMEX post-await disposal guard'),
]
selected = sys.argv[1:] or [row[0] for row in controls]
assert set(selected) <= {row[0] for row in controls}, 'unknown control'

def run(name, test, phase):
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-phemex', '--lib', f'binding::exchange::divergence_regressions::{test}', '--', '--exact']
    result = subprocess.run(command, cwd=workspace, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / f'{name}-{phase}.log').write_bytes(result.stdout)
    return result

for name, file, mutate, test, assertion in controls:
    if name not in selected:
        continue
    path = crate / 'src' / file
    original = path.read_bytes()
    assert run(name, test, 'baseline').returncode == 0, name + ' baseline'
    try:
        modified = mutate(original.decode()).encode()
        assert modified != original
        path.write_bytes(modified)
        result = run(name, test, 'red')
        log = result.stdout.decode(errors='replace')
        assert result.returncode == 101 and 'Finished `test`' in log and 'panicked at' in log and assertion in log and 'test result: FAILED' in log, name + ' requires compiled named assertion RED: ' + log[-3000:]
    finally:
        path.write_bytes(original)
        assert path.read_bytes() == original
    assert run(name, test, 'restored').returncode == 0, name + ' restored'
    print(name + ': compiled RED -> exact restore GREEN', flush=True)
