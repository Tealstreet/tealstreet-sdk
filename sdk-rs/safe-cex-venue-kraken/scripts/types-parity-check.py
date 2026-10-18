#!/usr/bin/env python3
"""Run the maintained Rust indexer on this crate, implemented rows, and owned FINDING rows."""
from pathlib import Path
import json
import subprocess
import tempfile
import tomllib

workspace = Path(__file__).resolve().parents[2]


def targets(row):
    return row['rs'] if isinstance(row['rs'], list) else [row['rs']]


def owned_rows(symbols):
    """Keep complete scalar/split claims; never derive or trim target mappings."""
    return [row for row in symbols
            if any(target.startswith('safe-cex-venue-kraken/')
                   for target in targets(row))
            or (not row['rs'] and row['note'].startswith('FINDING:'))]


def declared_files(files, rows):
    selected = {target.split('::', 1)[0] for row in rows for target in targets(row)}
    # Intersect committed declarations; never manufacture one from an rs target.
    # Keep all Kraken declarations, including unused ones the checker must reject.
    return [file for file in files if file.startswith('safe-cex-venue-kraken/') or file in selected]


def main():
    # Retain full dependency/reexport context; narrow only invention obligations.
    with tempfile.TemporaryDirectory(prefix='kraken-types-index-', dir=workspace / 'target') as directory:
        scoped = Path(directory)
        manifests = scoped / 'manifests'
        manifests.mkdir()
        # The crate now includes API/socket modules beyond the original seven-file slice.
        # Index every committed Kraken manifest so their public items remain gated.
        for manifest in sorted((workspace / 'parity/manifest/exchanges/kraken').glob('*.ts.toml')):
            filename = manifest.name
            original = tomllib.loads(manifest.read_text())
            # Keep all members of split claims, and owned unmapped FINDINGs.
            # Other pending hooks and inherited shared-crypto mappings remain outside.
            rows = owned_rows(original['symbol'])
            rust_files = declared_files(original['file'].get('rust_files', []), rows)
            text = '[file]\nts = ' + json.dumps(original['file']['ts']) + '\nrust_files = ' + json.dumps(rust_files) + '\n'
            for row in rows:
                text += '\n[[symbol]]\n' + ''.join(key + ' = ' + json.dumps(value) + '\n' for key, value in row.items())
            (manifests / filename).write_text(text)
        exempt = scoped / 'exempt.toml'
        exempt.write_text('exempt = []\n')
        subprocess.run([str(workspace / 'target/debug/parity-check'), '--workspace', str(workspace),
                        '--invention-crate', 'safe-cex-venue-kraken',
                        '--manifest', str(manifests), '--exempt', str(exempt)], check=True)


if __name__ == "__main__":
    main()
