#!/usr/bin/env python3
"""Require compiled real-socket timing failures and exact restored passes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

venue = Path(__file__).resolve().parents[1]
sdk = venue.parent
protocol = venue / 'src/ws_public/topics.rs'
lifecycle = venue / 'src/public_ws_lifecycle.rs'
original = {p: p.read_bytes() for p in [protocol, lifecycle]}
subscribe = 'dedicated_subscribe_starts_before_ping_and_retains_ten_ms_continuation'
markets = 'market_additions_wait_five_ms_between_real_socket_chunks'
controls = [
    ('deferred-prefix', protocol, subscribe, [
        ('let operation = lifecycle::subscribe(', 'let operation = async move { lifecycle::subscribe('),
        ('move |ms| runtime.sleep(ms),\n        );', 'move |ms| runtime.sleep(ms),\n        ).await };'),
    ]),
    ('zero-subscribe-delay', lifecycle, subscribe, [
        ('Ok(Some((frames, sleep(10))))', 'Ok(Some((frames, sleep(0))))'),
    ]),
    ('dropped-subscribe-tail', protocol, subscribe, [
        ('socket.start_message_operation(operation);\n        Ok(())', 'drop(operation);\n        Ok(())'),
    ]),
    ('dropped-market-tail', protocol, markets, [
        ('socket.start_message_operation(operation);\n            Ok(())', 'drop(operation);\n            Ok(())'),
    ]),
]


def run(label, test):
    command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-bybit',
               '--all-features', '--test', 'public_topic_binding', test, '--', '--include-ignored']
    result = subprocess.run(command, cwd=sdk, env=dict(os.environ, CARGO_INCREMENTAL='0'),
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    Path('/tmp/bybit-public-scheduling-' + label + '.log').write_bytes(result.stdout)
    return result


records = []
try:
    for label, path, test, edits in controls:
        baseline = run(label + '-baseline', test)
        assert baseline.returncode == 0 and b'1 passed; 0 failed' in baseline.stdout, label
        source = original[path].decode()
        for before, after in edits:
            assert source.count(before) == 1, (label, before)
            source = source.replace(before, after)
        path.write_text(source)
        red = run(label + '-red', test)
        output = red.stdout.decode()
        assert red.returncode == 101 and 'panicked at' in output and test + ' ... FAILED' in output and 'error[E' not in output, label
        path.write_bytes(original[path])
        assert all(p.read_bytes() == data for p, data in original.items())
        green = run(label + '-restored', test)
        assert green.returncode == 0 and b'1 passed; 0 failed' in green.stdout, label
        records.append(dict(control=label, test=test, red_sha256=hashlib.sha256(red.stdout).hexdigest(),
                            green_sha256=hashlib.sha256(green.stdout).hexdigest(), exact_restore=True))
        print(label + ': compiled named RED -> exact restored GREEN', flush=True)
finally:
    for path, data in original.items():
        path.write_bytes(data)
assert all(p.read_bytes() == data for p, data in original.items())
Path('/tmp/bybit-public-scheduling-controls.json').write_text(json.dumps(records, indent=2) + '\n')
