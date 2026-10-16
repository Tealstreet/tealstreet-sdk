"""Run canonical parity on the Lighter crate, including read-only SDK mappings."""
from pathlib import Path
import json
import os
import subprocess
import tomllib

sdk = Path(__file__).resolve().parents[2]
crate = 'safe-cex-venue-lighter'
scope = sdk / 'target/lighter-approval-parity-scope'
scope.mkdir(parents=True, exist_ok=True)
roots = [sdk / 'parity/manifest' / relative for relative in ('exchanges/lighter', 'lighter-sdk')]
members = {crate}
for root in roots:
    for path in root.rglob('*.toml'):
        for row in tomllib.loads(path.read_text())['symbol']:
            targets = row['rs'] if isinstance(row['rs'], list) else [row['rs']]
            members.update(target.split('/')[0] for target in targets if target)
for member in sorted(members):
    assert (sdk / member / 'Cargo.toml').is_file(), member
    link = scope / member
    if not link.exists():
        link.symlink_to(sdk / member, target_is_directory=True)
workspace = tomllib.loads((sdk / 'Cargo.toml').read_text())
dependencies = workspace['workspace']['dependencies']
lines = [f'[workspace]\nmembers = {json.dumps(sorted(members))}\n[workspace.dependencies]']
for name, value in dependencies.items():
    if isinstance(value, dict) and 'path' in value:
        lines.append(f'{name} = {{ path = {json.dumps(str(sdk / value["path"]))} }}')
    elif isinstance(value, str):
        lines.append(f'{name} = {json.dumps(value)}')
    elif isinstance(value, dict) and 'version' in value:
        fields = ', '.join(f'{key} = {json.dumps(item)}' for key, item in value.items() if key in ('version', 'features'))
        lines.append(f'{name} = {{ {fields} }}')
(scope / 'Cargo.toml').write_text('\n'.join(lines) + '\n')
manifest = scope / 'manifest'
manifest.mkdir(exist_ok=True)
for name, relative in [('venue', 'exchanges/lighter'), ('sdk', 'lighter-sdk')]:
    link = manifest / name
    if not link.exists():
        link.symlink_to(sdk / 'parity/manifest' / relative, target_is_directory=True)
rows = [row for row in tomllib.loads((sdk / 'parity/exempt.toml').read_text())['exempt'] if row['rs'].split('/')[0] in members]
(scope / 'exempt.toml').write_text('exempt = []\n' if not rows else '\n'.join('[[exempt]]\n' + '\n'.join(f'{key} = {json.dumps(value)}' for key, value in row.items()) for row in rows))
print(f'Lighter member floor: {sorted(members)}; exemption floor: {len(rows)} blocks', flush=True)
subprocess.run([os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'run', '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-parity', '--jobs', '1', '--bin', 'parity-check', '--', '--workspace', str(scope), '--manifest', str(manifest), '--exempt', str(scope / 'exempt.toml')], check=True)
