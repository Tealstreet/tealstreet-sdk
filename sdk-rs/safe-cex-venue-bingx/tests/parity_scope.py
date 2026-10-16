#!/usr/bin/env python3
"""Run the real parity indexer against only BingX production declarations/rows."""
from pathlib import Path
import json
import subprocess
import tempfile
import tomllib

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
build = subprocess.run([cargo, 'build', '--manifest-path', str(sdk / 'Cargo.toml'),
                '-p', 'safe-cex-parity', '--bin', 'parity-check', '--jobs', '1', '--locked',
                '--target-dir', str(sdk / 'target'), '--message-format=json'],
               check=True, text=True, stdout=subprocess.PIPE)
artifacts = [json.loads(line) for line in build.stdout.splitlines()]
executables = [row['executable'] for row in artifacts
               if row.get('reason') == 'compiler-artifact'
               and row.get('target', {}).get('name') == 'parity-check'
               and row.get('executable')]
assert len(executables) == 1, 'Cargo must report exactly one parity-check executable'
with tempfile.TemporaryDirectory(prefix='bingx-parity-', dir=sdk / 'target') as directory:
    scope = Path(directory)
    (scope / crate.name).symlink_to(crate, target_is_directory=True)
    (scope / 'Cargo.toml').write_text(
        '[workspace]\nmembers = ["safe-cex-venue-bingx"]\n[workspace.dependencies]\n' +
        ''.join(f'{name} = {{ path = {json.dumps(str(sdk / name))} }}\n'
                for name in ['safe-cex-types', 'safe-cex-num', 'safe-cex-exchange', 'safe-cex-transport', 'safe-cex-store']) + '\nserde = "1"\nserde_json = "1"\n')
    exemptions = tomllib.loads((sdk / 'parity/exempt.toml').read_text())['exempt']
    (scope / 'exempt.toml').write_text(''.join(
        f'[[exempt]]\nrs = {json.dumps(row["rs"])}\nreason = {json.dumps(row["reason"])}\n\n'
        for row in exemptions if row['rs'].startswith('safe-cex-venue-bingx/')))
    subprocess.run([executables[0], '--workspace', str(scope),
                    '--manifest', str(sdk / 'parity/manifest/exchanges/bingx'),
                    '--exempt', str(scope / 'exempt.toml')], check=True)
