#!/usr/bin/env python3
"""Run the canonical parity indexer against only this crate and its six manifests."""
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
CRATE = 'safe-cex-venue-cryptocom'
CARGO = '/opt/homebrew/opt/rustup/bin/cargo'
subprocess.run([CARGO, 'build', '--manifest-path', str(SDK / 'Cargo.toml'),
                '-p', 'safe-cex-parity', '--bin', 'parity-check', '--jobs', '1', '--locked'], check=True)
# The canonical CLI has a workspace selector, but no crate filter. Give it an
# isolated one-member source view; do not suppress inventions after indexing.
with tempfile.TemporaryDirectory(prefix='cryptocom-parity-', dir=SDK / 'target') as temporary:
    view = Path(temporary)
    (view / CRATE).symlink_to(SDK / CRATE, target_is_directory=True)
    (view / 'safe-cex-num').symlink_to(SDK / 'safe-cex-num', target_is_directory=True)
    for name in ['safe-cex-crypto', 'safe-cex-exchange', 'safe-cex-store']:
        (view / name).symlink_to(SDK / name, target_is_directory=True)
    (view / 'safe-cex-types').symlink_to(SDK / 'safe-cex-types', target_is_directory=True)
    (view / 'Cargo.toml').write_text(
        '[workspace]\nmembers = ["safe-cex-venue-cryptocom"]\n'
        '[workspace.dependencies]\nsafe-cex-num = { path = "safe-cex-num" }\n'
        'safe-cex-types = { path = "safe-cex-types" }\n'
        'safe-cex-crypto = { path = "safe-cex-crypto" }\n'
        'safe-cex-exchange = { path = "safe-cex-exchange" }\n'
        'safe-cex-store = { path = "safe-cex-store" }\n'
        'serde = "' + tomllib.loads((SDK / 'Cargo.toml').read_text())['workspace']['dependencies']['serde']['version'] + '"\n'
        'serde_json = "' + tomllib.loads((SDK / 'Cargo.toml').read_text())['workspace']['dependencies']['serde_json']['version'] + '"\n')
    (view / 'exempt.toml').write_text('exempt = []\n')
    subprocess.run([str(SDK / 'target/debug/parity-check'), '--workspace', str(view),
                    '--manifest', str(SDK / 'parity/manifest/exchanges/cryptocom'),
                    '--exempt', str(view / 'exempt.toml')], check=True)
rows = [row for file in (SDK / 'parity/manifest/exchanges/cryptocom').glob('*.toml')
        for row in tomllib.loads(file.read_text())['symbol']]
print('tomllib total:', len(rows), 'ported/divergent/pending:',
      tuple(sum(row['status'] == status for row in rows) for status in ['ported', 'divergent', 'pending']))
