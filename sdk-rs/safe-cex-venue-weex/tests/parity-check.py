"""Run the canonical parity gate on WEEX only, without mutating shared inputs."""
from pathlib import Path
import collections
import json
import os
import subprocess
import tomllib

sdk = Path(__file__).resolve().parents[2]
crate = 'safe-cex-venue-weex'
scope = sdk / 'target/weex-parity-scope'
scope.mkdir(parents=True, exist_ok=True)
link = scope / crate
if not link.exists():
    link.symlink_to(sdk / crate, target_is_directory=True)
(scope / 'Cargo.toml').write_text(
    f'[workspace]\nmembers = ["{crate}"]\n[workspace.dependencies]\n'
    f'safe-cex-types = {{ path = {json.dumps(str(sdk / "safe-cex-types"))} }}\n'
    f'safe-cex-store = {{ path = {json.dumps(str(sdk / "safe-cex-store"))} }}\n'
    f'safe-cex-num = {{ path = {json.dumps(str(sdk / "safe-cex-num"))} }}\n'
    f'safe-cex-exchange = {{ path = {json.dumps(str(sdk / "safe-cex-exchange"))} }}\n'
    + ''.join(f'{name} = {json.dumps(value["version"] if isinstance(value, dict) else value)}\n'
        for name, value in tomllib.loads((sdk / 'Cargo.toml').read_text())['workspace']['dependencies'].items()
        if name in ['serde_json', 'serde', 'hmac', 'sha2', 'base64']))
exemptions = tomllib.loads((sdk / 'parity/exempt.toml').read_text())['exempt']
scoped = [row for row in exemptions if row['rs'].startswith(crate + '/')]
print(f'GLOBAL exemption BLOCKS={len(exemptions)} DISTINCT={len({r["rs"] for r in exemptions})}', flush=True)
print(f'WEEX exemption BLOCKS={len(scoped)} DISTINCT={len({r["rs"] for r in scoped})}', flush=True)
(scope / 'exempt.toml').write_text('exempt = []\n' if not scoped else '\n'.join(
    '[[exempt]]\n' + '\n'.join(f'{k} = {json.dumps(v)}' for k, v in row.items()) for row in scoped))
counts = collections.Counter()
for path in (sdk / 'parity/manifest/exchanges/weex').glob('*.toml'):
    counts.update(row['status'] for row in tomllib.loads(path.read_text())['symbol'])
print(f'WEEX tomllib counts: {dict(counts)} TOTAL={sum(counts.values())}', flush=True)
subprocess.run([os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'run',
    '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-parity', '--jobs', '1',
    '--bin', 'parity-check', '--', '--workspace', str(scope),
    '--manifest', str(sdk / 'parity/manifest/exchanges/weex'),
    '--exempt', str(scope / 'exempt.toml')], check=True)
