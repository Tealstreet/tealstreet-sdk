"""Run the existing parity checker over BloFin source and manifests only."""
import pathlib
import json
import tomllib
import re
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[3]
sdk = root / 'sdk-rs'
# Changed-file source coverage is separate from invention/status checking.
coverage = json.loads(subprocess.check_output(
    ['node', str(sdk / 'safe-cex-venue-blofin/tests/coverage_scope.cjs')], cwd=root, text=True))
for basename, source_rows in coverage.items():
    manifest_rows = tomllib.loads((sdk / 'parity/manifest/exchanges/blofin' / (basename + '.toml')).read_text())['symbol']
    assert source_rows == [{'ts': row['ts'], 'kind': row['kind']} for row in manifest_rows], basename + '_source_coverage'
    print(f'Changed-file coverage {basename}: {len(source_rows)} declarations; 0 drift', flush=True)
# This is an indexer input, not a Cargo build workspace. Link production sources;
# no copied Rust declarations or invented symbol resolver can hide source drift.
with tempfile.TemporaryDirectory(prefix='blofin-parity-', dir=sdk / 'target') as folder:
    scope = pathlib.Path(folder)
    config = (sdk / 'Cargo.toml').read_text()
    config = re.sub(r'members = \[.*?\]', 'members = ["safe-cex-venue-blofin"]', config, count=1, flags=re.S)
    (scope / 'Cargo.toml').write_text(config)
    for name in ['safe-cex-venue-blofin', 'safe-cex-types', 'safe-cex-num', 'safe-cex-store', 'safe-cex-transport']:
        (scope / name).symlink_to(sdk / name, target_is_directory=True)
    (scope / 'exempt.toml').write_text('')
    subprocess.run([
        '/opt/homebrew/opt/rustup/bin/cargo', 'run', '--manifest-path', str(sdk / 'Cargo.toml'),
        '--locked', '-p', 'safe-cex-parity', '--jobs', '1', '--bin', 'parity-check', '--',
        '--workspace', str(scope), '--manifest', str(sdk / 'parity/manifest/exchanges/blofin'),
        '--exempt', str(scope / 'exempt.toml'),
    ], cwd=root, check=True)
