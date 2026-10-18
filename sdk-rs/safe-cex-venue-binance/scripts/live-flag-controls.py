#!/usr/bin/env python3
"""Prove native retry guards and owned Cell conversions are observed."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/class_flags.rs'
original = source.read_bytes()
logs = crate.parent / 'target/live-flag-controls'
logs.mkdir(parents=True, exist_ok=True)
cases = [
    ('live-reader', b'Self::Read(get) => get(),', b'Self::Read(_) => false,',
     'live_flag_binding::production_native_retry_flags_read_live_owner_after_held_http',
     'retry flag observes actual Base disposal'),
    ('owned-cell', b'Self::Cell(cell)\n    }\n}', b'Self::Cell(Cell::new(!cell.get()))\n    }\n}',
     'live_flag_binding::production_flag_cell_conversion_retains_each_owned_boolean',
     'actual Cell conversion preserves the initial boolean'),
]


def execute(label, name):
    path = logs / (label + '.log')
    command = ['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1',
               '-p', 'safe-cex-venue-binance', '--features', 'native', '--test', 'private_classes',
               name, '--', '--exact']
    with path.open('w') as output:
        result = subprocess.run(command, cwd=crate.parent,
                                env={**os.environ, 'CARGO_BUILD_JOBS': '1'},
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text()


for label, before, after, name, assertion in cases:
    assert original.count(before) == 1, label
    try:
        source.write_bytes(original.replace(before, after))
        status, log = execute(label + '-red', name)
        assert status != 0 and f'test {name} ... FAILED' in log, log[-5000:]
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), log[-5000:]
        assert assertion in log, log[-5000:]
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log = execute(label + '-restored', name)
    assert status == 0 and '1 passed; 0 failed' in log, log[-5000:]
    print(f'{label}: compiled assertion RED; exact restoration GREEN.', flush=True)
