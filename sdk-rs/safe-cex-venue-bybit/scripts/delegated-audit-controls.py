#!/usr/bin/env python3
"""Compile faults in the actual reconnect/SDK/control bindings, then restore."""
from pathlib import Path
import hashlib
import json
import os
import subprocess

venue = Path(__file__).resolve().parents[1]
sdk = venue.parent
private = venue / 'src/ws_private.rs'
facade = venue / 'src/binding/exchange.rs'
controls = venue / 'src/binding/members/controls.rs'
original = {p: p.read_bytes() for p in [private, facade, controls]}
reconnect_test = 'delegated_inverse_and_spot_reconnect_start_eagerly_through_real_protocol'
controls_test = 'delegated_controls_preserve_eager_prefix_and_dropped_waiter_continuations'
unit = '''        let runtime = self.rt().exchange_runtime();
        let result = self.rt().this.call_method(member, args);
        Box::pin(async move {
            await_js(&runtime, result.map_err(err)?)'''
faults = [
    ('missing-reconnect-prefix', private, reconnect_test,
     'let sync = sync_clock();',
     'let sync = { drop(sync_clock); std::future::ready(Ok::<(), ExchangeError>(())) };',
     'eager clock'),
    ('forced-auth-refusal', private, reconnect_test,
     'match auth().await {', 'match { drop(auth); Ok::<bool, ExchangeError>(false) } {',
     'composed auth frames'),
    ('deferred-sdk-unit-call', facade, controls_test, unit,
     '''        let runtime = self.rt().exchange_runtime();
        let this = self.rt().this.clone();
        Box::pin(async move {
            let result = this.call_method(member, args);
            await_js(&runtime, result.map_err(err)?)''',
     'source/native prefix'),
    ('lost-mode-refresh-publication', controls, controls_test,
     '|positions| store_call(&rt, "update", Js::object(vec![("positions", positions)])),\n            |positions| store_call(&rt, "addOrUpdatePositions", positions),',
     '|_positions| Ok(()),\n            |positions| store_call(&rt, "addOrUpdatePositions", positions),', 'source/native afterAll'),
]
records = []

def run(label, test):
    log = Path('/tmp/bybit-final-delegated-' + label + '.log')
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-bybit',
               '--all-features', '--lib', test, '--', '--include-ignored']
    with log.open('w') as stream:
        result = subprocess.run(command, cwd=sdk,
                                env=dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2'),
                                stdout=stream, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, log, log.read_text()

try:
    for label, path, test, before, after, assertion in faults:
        text = original[path].decode()
        assert text.count(before) == 1, (label, text.count(before))
        path.write_text(text.replace(before, after, 1))
        code, red_log, red = run(label + '-red', test)
        assert code == 101 and 'error[E' not in red and 'test result: FAILED' in red and assertion in red, (label, red_log)
        path.write_bytes(original[path])
        assert path.read_bytes() == original[path]
        code, green_log, green = run(label + '-restore-green', test)
        assert code == 0 and '1 passed; 0 failed' in green, (label, green_log)
        record = dict(control=label, compiled_named_red=True, assertion=assertion,
                      exact_restore=True, green=True, red_log=str(red_log), green_log=str(green_log),
                      red_sha256=hashlib.sha256(red_log.read_bytes()).hexdigest(),
                      green_sha256=hashlib.sha256(green_log.read_bytes()).hexdigest())
        records.append(record)
        print(json.dumps(record), flush=True)
        Path('/tmp/bybit-final-delegated-controls.json').write_text(json.dumps(records, indent=2) + '\n')
finally:
    for path, content in original.items():
        path.write_bytes(content)
assert all(path.read_bytes() == content for path, content in original.items())
