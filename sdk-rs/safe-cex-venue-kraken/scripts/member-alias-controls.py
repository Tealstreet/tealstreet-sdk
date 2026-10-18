#!/usr/bin/env python3
"""Compiled faults in the production class-member continuation consumers."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix='kraken-member-alias-controls-'))
path = sdk / 'safe-cex-venue-kraken/src/binding/members.rs'
cases = [
    ('BodyFuture', 'body_future_keeps_held_http_and_completed_native_candle_fields',
     'Box::pin(async move { body.await.map(h) })',
     'Box::pin(async move { let _ = body.await?; Ok(h(Js::Undefined)) })'),
    ('NativeMember', 'native_member_uses_supplied_constructed_receiver_and_original_arguments',
     'dispatch(rt.with_this((*receiver).clone()), target, args)',
     'dispatch(rt.with_this({ let _ = receiver; Js::Undefined }), target, args)'),
    ('BoundMember', 'bound_member_arrow_keeps_captured_receiver_arguments_and_promise_result',
     'Ok(match body(args) {', 'Ok(match body({ let _ = args; Vec::new() }) {'),
]
results = []
for alias, test, needle, replacement in cases:
    original = path.read_bytes()
    source = original.decode()
    assert source.count(needle) == 1
    name = f'binding::exchange::accepted_member_aliases::{test}'
    def run(label, negative):
        result = subprocess.run([
            'cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken',
            '--lib', name, '--', '--exact', '--nocapture',
        ], cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f'{alias}-{label}.log').write_text(result.stdout)
        if negative:
            assert result.returncode != 0 and f'test {name} ... FAILED' in result.stdout and 'panicked at' in result.stdout and 'could not compile' not in result.stdout, result.stdout
        else:
            assert result.returncode == 0 and f'test {name} ... ok' in result.stdout, result.stdout
        return result.returncode
    try:
        path.write_text(source.replace(needle, replacement))
        red = run('red', True)
    finally:
        path.write_bytes(original)
    green = run('restored', False)
    results.append({'alias': alias, 'test': name, 'compiledRed': red, 'exactRestoredGreen': green})
    (receipts / 'receipts.json').write_text(json.dumps(results, indent=2)+'\n')
    print(f'{alias}: named compiled RED/exact-restored GREEN', flush=True)
print(receipts, flush=True)
