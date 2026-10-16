#!/usr/bin/env python3
"""Run the maintained Rust indexer on this crate, implemented rows, and owned FINDING rows."""
from pathlib import Path
import re
import json
import subprocess
import tempfile
import tomllib

workspace = Path(__file__).resolve().parents[2]
source = (workspace / 'Cargo.toml').read_text()
config = tomllib.loads(source)
# The temporary workspace narrows source-index discovery, not Cargo compilation.
with tempfile.TemporaryDirectory(prefix='kraken-types-index-', dir=workspace / 'target') as directory:
    scoped = Path(directory)
    scoped.joinpath('Cargo.toml').write_text(re.sub(r'members = \[.*?\n\]', 'members = ["safe-cex-venue-kraken"]', source, count=1, flags=re.S))
    for member in config['workspace']['members']:
        scoped.joinpath(member).symlink_to(workspace / member, target_is_directory=True)
    manifests = scoped / 'manifests'
    manifests.mkdir()
    for name in ['kraken.types', 'krakenspot.types', 'kraken.fills', 'kraken.income', 'kraken.api', 'kraken.exchange', 'krakenspot.exchange']:
        filename = name + '.ts.toml'
        original = tomllib.loads((workspace / 'parity/manifest/exchanges/kraken' / filename).read_text())
        # Include owned unmapped FINDINGs so the status gate cannot hide missing work.
        # Other pending hooks and the inherited shared-crypto mapping remain outside this gate.
        rows = [row for row in original['symbol'] if row['rs'].startswith('safe-cex-venue-kraken/')
                or (not row['rs'] and row['note'].startswith('FINDING:'))]
        rust_files = [file for file in original['file'].get('rust_files', []) if file.startswith('safe-cex-venue-kraken/')]
        text = '[file]\nts = ' + json.dumps(original['file']['ts']) + '\nrust_files = ' + json.dumps(rust_files) + '\n'
        for row in rows:
            text += '\n[[symbol]]\n' + ''.join(key + ' = ' + json.dumps(value) + '\n' for key, value in row.items())
        (manifests / filename).write_text(text)
    exempt = scoped / 'exempt.toml'
    exempt.write_text('exempt = []\n')
    subprocess.run([str(workspace / 'target/debug/parity-check'), '--workspace', str(scoped),
                    '--manifest', str(manifests), '--exempt', str(exempt)], check=True)
