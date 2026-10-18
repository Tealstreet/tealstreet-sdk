"""Compiled production controls for actual Apex/BloFin retained-info bindings."""
from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
cases = [
    ('apex-position-info', 'safe-cex-venue-apexomni', 'binding',
     'private_frame_info_keeps_opaque_utf16_and_wire_numbers', 'src/ws_private.rs',
     'patch.set_native("info", info)', 'patch.set_native("info", { let _ = info; Input::Null })'),
    ('blofin-ticker-info', 'safe-cex-venue-blofin', 'reads',
     'native_ticker_fetch_retains_opaque_info_before_projection', 'src/tickers.rs',
     'info: Some(row.clone())', 'info: Some(Input::Null)'),
]
for name, package, target, test, filename, before, after in cases:
    source = workspace / package / filename
    original = source.read_bytes()
    assert original.decode().count(before) == 1
    command = ['cargo', 'test', '-p', package, '--jobs', '1', '--test', target, test, '--', '--nocapture']
    try:
        source.write_text(original.decode().replace(before, after))
        result = subprocess.run(command, cwd=workspace, env=os.environ, capture_output=True, text=True)
        log = result.stdout + result.stderr
        Path('/tmp/' + name + '-control-red.log').write_text(log)
        assert result.returncode == 101 and 'test result: FAILED' in log, log
        assert test in log, log
        print(name, 'compiled RED', flush=True)
    finally:
        source.write_bytes(original)
    result = subprocess.run(command, cwd=workspace, env=os.environ, capture_output=True, text=True)
    log = result.stdout + result.stderr
    Path('/tmp/' + name + '-control-restored.log').write_text(log)
    assert result.returncode == 0 and '1 passed' in log, log
    print(name, 'exact restore GREEN', flush=True)
