"""Require each compiling heartbeat mutation to fail a named source assertion."""
from pathlib import Path
import os
import subprocess

path = Path('sdk-rs/safe-cex-venue-woo/src/public_socket.rs')
original = path.read_text()
ping_send = 'socket.send(r#"{"event":"ping"}"#)'
cases = [
    ('disposed-clock', 'if socket.is_disposed() {', 'if false && socket.is_disposed() {', 'heartbeat ping timestamp'),
    ('missing-clock', 'socket.mark_ping_sent();', '', 'heartbeat ping timestamp'),
    ('clock-after-send', 'socket.mark_ping_sent();\n    ' + ping_send, ping_send + '?;\n    socket.mark_ping_sent();\n    Ok(())', 'heartbeat ping timestamp'),
    ('swallowed-send', ping_send, ping_send + '.or(Ok(()))', 'heartbeat error'),
    ('pong-payload', 'socket.send(r#"{"event":"pong"}"#)', ping_send, 'heartbeat payload'),
]
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
for name, before, after, assertion in cases:
    assert before in original, name
    try:
        path.write_text(original.replace(before, after, 1))
        run = subprocess.run([
            'cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
            '--locked', '--offline', '--jobs', '2', '-p', 'safe-cex-venue-woo',
            '--test', 'public_heartbeats',
        ], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path(f'/tmp/woo-heartbeat-control-{name}.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)
