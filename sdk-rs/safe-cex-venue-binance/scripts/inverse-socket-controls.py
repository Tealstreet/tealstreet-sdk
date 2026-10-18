#!/usr/bin/env python3
"""Compile one real socket fault at a time; restore exact production bytes."""
from pathlib import Path
import os
import subprocess
import sys

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-binance'
cases = [
    ('stale-position-version', 'ws_private.rs',
     'websocket_version_prevents_late_rest_position_snapshot_overwrite',
     'version.strict_equals(&rt.call_path("parent.getPositionStateVersion", vec![])?)',
     '{ let _ = &version; true }'),
    ('lexical-receiver', 'ws_public.rs',
     'public_wire_topics_and_all_three_ticker_streams_reach_real_store',
     'move |_, args| {\n                let this = Js::Object(\n                    lexical\n                        .upgrade()\n                        .ok_or_else(|| Thrown::error("Error", "socket receiver dropped"))?,\n                );',
     'move |this, args| { let _ = &lexical;'),
    ('depth-sequence-gap', 'ws_public.rs',
     'depth_buffers_snapshot_requires_previous_id_and_resyncs_gap',
     '&mut self.sync.borrow_mut(),\n            true,',
     '&mut self.sync.borrow_mut(),\n            false,'),
    ('native-fill-units', 'ws_private.rs',
     'private_stream_preserves_big_ids_raw_fill_units_and_terminal_reason',
     '("amount", Js::Number(data.get("l")?.parse_float()?)),',
     '("amount", Js::Number(data.get("l")?.parse_float()? / 50000.)),'),
    ('retry-backoff', 'ws_private.rs',
     'listen_key_retry_backoff_caps_and_real_force_close_blocks_retry',
     'Js::Number((delay * 2.).min(60_000.)),',
     'Js::Number((delay * 3.).min(60_000.)),'),
]
if len(sys.argv) > 1:
    cases = [case for case in cases if case[0] == sys.argv[1]]
    assert len(cases) == 1, 'unknown socket control'
originals = {crate / 'src/inverse' / file: (crate / 'src/inverse' / file).read_bytes()
             for _, file, _, _, _ in cases}
env = dict(os.environ, RUSTC_BOOTSTRAP='1')

def run(test, label):
    command = ['cargo', 'test', '--offline', '-p', 'safe-cex-venue-binance', '--jobs', '1',
               '--test', 'inverse_socket_binding', test, '--', '--exact']
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, text=True, timeout=900)
    output = result.stdout + result.stderr
    Path('/tmp/inverse-socket-control-' + label + '.log').write_text(output)
    return result.returncode, output

try:
    for label, file, test, before, after in cases:
        path = crate / 'src/inverse' / file
        original = originals[path]
        assert original.count(before.encode()) == 1, label
        path.write_bytes(original.replace(before.encode(), after.encode()))
        code, output = run(test, label + '-red')
        assert code != 0 and f'test {test} ... FAILED' in output and 'error[E' not in output, output[-6000:]
        print(f'COMPILED RED {label}: {test}', flush=True)
        path.write_bytes(original)
        code, output = run(test, label + '-green')
        assert code == 0 and '1 passed; 0 failed' in output, output[-6000:]
        print(f'RESTORED GREEN {label}: {test}', flush=True)
finally:
    for path, source in originals.items():
        path.write_bytes(source)
