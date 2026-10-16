"""Run production parity checker against only this venue's actual source/manifests."""
from pathlib import Path
import re
import tomllib
import subprocess
import tempfile

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
scratch = workspace / 'target'
scratch.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='orderly-parity-', dir=scratch) as directory:
    scope = Path(directory)
    (scope / crate.name).symlink_to(crate, target_is_directory=True)
    # Retain actual inherited dependency declarations; only narrow the member set.
    config, replacements = re.subn(r'(?ms)^members = \[.*?^\]',
        'members = ["safe-cex-venue-orderly"]', (workspace / 'Cargo.toml').read_text(), count=1)
    assert replacements == 1
    assert tomllib.loads(config)['workspace']['members'] == ['safe-cex-venue-orderly']
    (scope / 'Cargo.toml').write_text(config)
    (scope / 'exempt.toml').write_text('')
    subprocess.run([
        '/opt/homebrew/opt/rustup/bin/cargo', 'run', '--manifest-path', str(workspace / 'Cargo.toml'),
        '-p', 'safe-cex-parity', '--bin', 'parity-check', '--jobs', '1', '--locked', '--',
        '--workspace', str(scope), '--manifest', str(workspace / 'parity/manifest/exchanges/orderly'),
        '--exempt', str(scope / 'exempt.toml'),
    ], check=True)
