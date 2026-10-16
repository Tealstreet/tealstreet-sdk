"""Run the real parity indexer against only this venue's sources and manifests."""
from pathlib import Path
import subprocess
import tomllib
import json

root = Path(__file__).resolve().parents[3]
sdk = root / 'sdk-rs'
scope = sdk / 'target/bitunix-parity-scope'
scope.mkdir(parents=True, exist_ok=True)
member = scope / 'safe-cex-venue-bitunix'
if not member.exists():
    member.symlink_to(sdk / 'safe-cex-venue-bitunix', target_is_directory=True)
def toml_value(value):
    if isinstance(value, dict):
        return '{ ' + ', '.join(json.dumps(k) + ' = ' + toml_value(v) for k, v in value.items()) + ' }'
    if isinstance(value, list):
        return '[' + ', '.join(toml_value(v) for v in value) + ']'
    return json.dumps(value)

workspace = tomllib.loads((sdk / 'Cargo.toml').read_text())['workspace']['dependencies']
config = tomllib.loads((sdk / 'safe-cex-venue-bitunix/Cargo.toml').read_text())
lines = ['[workspace]', 'members = ["safe-cex-venue-bitunix"]', '[workspace.dependencies]']
for name, declaration in config['dependencies'].items():
    if isinstance(declaration, dict) and declaration.get('workspace'):
        value = workspace[name]
        if isinstance(value, dict) and 'path' in value:
            value = dict(value, path=str(sdk / value['path']))
        lines.append(json.dumps(name) + ' = ' + toml_value(value))
(scope / 'Cargo.toml').write_text('\n'.join(lines) + '\n')
(scope / 'exempt.toml').write_text('')
manifests = scope / 'manifests'
manifests.mkdir(exist_ok=True)
for existing in manifests.glob('*.toml'):
    existing.unlink()
sources = sorted((sdk / 'parity/manifest/exchanges/bitunix').glob('*.toml'))
sources.append(sdk / 'parity/manifest/journal/bitunix.position-adapter.ts.toml')
for source in sources:
    (manifests / source.name).symlink_to(source)
subprocess.run([
    '/opt/homebrew/opt/rustup/bin/cargo', 'run', '--manifest-path', str(sdk / 'Cargo.toml'),
    '-p', 'safe-cex-parity', '--jobs', '1', '--bin', 'parity-check', '--',
    '--workspace', str(scope), '--manifest', str(manifests),
    '--exempt', str(scope / 'exempt.toml'),
], check=True)
