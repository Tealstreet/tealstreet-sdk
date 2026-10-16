"""Run the real parity indexer on only this venue, using original source symlinks."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import tomllib

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
cargo = os.environ.get('CARGO') or shutil.which('cargo')
assert cargo, 'cargo must be available through CARGO or PATH'
with tempfile.TemporaryDirectory(prefix='polymarket-parity-', dir=sdk / 'target') as temporary:
    scope = Path(temporary)
    text = (sdk / 'Cargo.toml').read_text()
    text, count = re.subn(r'members = \[.*?\]', 'members = ["safe-cex-venue-polymarket"]', text, count=1, flags=re.S)
    assert count == 1
    assert tomllib.loads(text)['workspace']['members'] == ['safe-cex-venue-polymarket']
    (scope / 'Cargo.toml').write_text(text)
    (scope / crate.name).symlink_to(crate, target_is_directory=True)
    # The indexer resolves dependency paths but these are deliberately not members.
    for dependency in ['safe-cex-types', 'safe-cex-num', 'safe-cex-exchange', 'safe-cex-transport', 'safe-cex-store', 'safe-cex-crypto', 'safe-cex-candle-cache']:
        (scope / dependency).symlink_to(sdk / dependency, target_is_directory=True)
    (scope / 'exempt.toml').write_text('exempt = []\n')
    subprocess.run([
        cargo, 'run', '--manifest-path', str(sdk / 'Cargo.toml'),
        '-p', 'safe-cex-parity', '--bin', 'parity-check', '--locked', '--jobs', '1', '--',
        '--workspace', str(scope), '--manifest', str(sdk / 'parity/manifest/exchanges/polymarket'),
        '--exempt', str(scope / 'exempt.toml'),
    ], check=True)
