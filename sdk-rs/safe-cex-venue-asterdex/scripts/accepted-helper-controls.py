#!/usr/bin/env python3
"""Compile isolated helper regressions; require native failures and exact restores."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-helper-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-helper-contracts.mjs']
CONTROLS = [
    ('stp-case-widening', 'helpers.rs',
     'ASTERDEX_STP_MODES.contains(&s)',
     'ASTERDEX_STP_MODES.iter().any(|mode| mode.eq_ignore_ascii_case(s))',
     'safe_cex_venue_asterdex::helpers::is_aster_dex_stp_mode'),
    ('nonpositive-server-time', 'clock_helpers.rs',
     '(parsed.is_finite() && parsed > 0.0).then_some(parsed)',
     '(parsed.is_finite() && parsed >= 0.0).then_some(parsed)',
     'safe_cex_venue_asterdex::clock_helpers::parse_server_time'),
]


def run(label, owner=None):
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler error is not evidence; {LOGS}'
    if owner is None:
        assert result.returncode == 0, f'{label}: exit {result.returncode}; {LOGS}'
        assert '18 actual production-native owners, zero failures' in output, LOGS
    else:
        assert result.returncode != 0, f'{label}: mutant escaped; {LOGS}'
        assert '18 production runtime results' in output, f'{label}: native execution required; {LOGS}'
        assert '0 explicit classifications; 1 failed owners' in output, f'{label}: exact failure census; {LOGS}'
        assert owner in output and 'AssertionError' in output, f'{label}: named owner assertion required; {LOGS}'
    print(f'{label}: exit {result.returncode}', flush=True)


run('baseline')
for label, filename, before, after, owner in CONTROLS:
    source = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src' / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, label
    try:
        source.write_text(text.replace(before, after))
        run(label, owner)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{label}-restored')
print(f'Two compiled helper faults caught; exact restores pass all 18 owners; logs {LOGS}')
