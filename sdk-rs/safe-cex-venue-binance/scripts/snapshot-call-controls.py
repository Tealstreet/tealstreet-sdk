#!/usr/bin/env python3
"""Require compiled native alias dataflow failures and exact source restoration."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / 'src/exchange_classes.rs'
original = source.read_bytes()
logs = workspace / 'target/snapshot-call-controls'
logs.mkdir(parents=True, exist_ok=True)
tests = {
    'LogSeverity': 'snapshot_call_binding::snapshot_call_keeps_source_log_error_severity',
    'ConsoleArguments': 'snapshot_call_binding::snapshot_call_keeps_variadic_console_native_values',
    'ConsoleError': 'snapshot_call_binding::snapshot_call_keeps_native_error_console_argument',
}
command = ['cargo', 'test', '--color', 'never', '--locked', '-p', 'safe-cex-venue-binance',
           '--features', 'native', '--jobs', '1', '--test', 'private_classes']
receipt = {'sourceSha256': hashlib.sha256(original).hexdigest(), 'controls': []}


def execute(label, name):
    path = logs / (label + '.log')
    with path.open('w') as output:
        result = subprocess.run(command + [name, '--', '--exact'], cwd=workspace, env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


for alias, before, after in [
    ('LogSeverity', 'owner.base.log(&value.text()?, &severity).map_err(Rc::new)',
     'owner.base.log(&value.text()?, { let _ = severity; "info" }).map_err(Rc::new)'),
    ('ConsoleArguments', 'owner.runtime.console_values("error", &values);',
     'owner.runtime.console_values("error", &values[..1]);'),
    ('ConsoleError', 'safe_cex_exchange::js_runtime::value::Thrown::from((*error).clone())\n                                    .value',
     'string(&error.message)'),
]:
    name = tests[alias]
    status, log = execute(alias + '-baseline-green', name)
    assert status == 0 and '1 passed; 0 failed' in log, log[-3000:]
    assert source.read_bytes() == original
    text = original.decode()
    assert text.count(before) == 1, alias
    try:
        source.write_text(text.replace(before, after))
        status, log = execute(alias + '-red', name)
        assert status != 0, alias + ': corrupted alias dataflow passed'
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), alias + ': compiler errors do not count'
        assert f'test {name} ... FAILED' in log, alias + ': named runtime assertion must fail'
        assert '0 passed; 1 failed' in log
        receipt['controls'].append({'owner': 'safe_cex_venue_binance::private_rest::SnapshotCall', 'role': alias,
                                    'test': name, 'namedRuntimeFailure': True})
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(alias + '-exact-restoration-green', name)
    assert status == 0 and '1 passed; 0 failed' in log, alias
    print(alias + ': compiled named native RED; exact restoration GREEN', flush=True)
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
assert source.read_bytes() == original
print('All three SnapshotCall controls fail for real and restore exactly.', flush=True)
