"""Official parity index/status gates for MEXC only; retain existing venue exemptions."""
from pathlib import Path
import json, re, subprocess, tempfile, tomllib
crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
with tempfile.TemporaryDirectory(prefix='mexc-parity-', dir=sdk/'target') as temporary:
    scope = Path(temporary)
    text = (sdk/'Cargo.toml').read_text()
    text, count = re.subn(r'members = \[.*?\]', 'members = ["safe-cex-venue-mexc"]', text, count=1, flags=re.S)
    assert count == 1
    assert tomllib.loads(text)['workspace']['members'] == ['safe-cex-venue-mexc']
    (scope/'Cargo.toml').write_text(text)
    (scope/crate.name).symlink_to(crate, target_is_directory=True)
    for dependency in ['safe-cex-types', 'safe-cex-num', 'safe-cex-store', 'safe-cex-exchange', 'safe-cex-transport', 'safe-cex-crypto', 'safe-cex-candle-cache']:
        (scope/dependency).symlink_to(sdk/dependency, target_is_directory=True)
    # Consume the governed declarations; do not fabricate or broaden exemptions.
    exemptions = [row for row in tomllib.loads((sdk/'parity/exempt.toml').read_text())['exempt'] if row['rs'].startswith(crate.name+'/')]
    text = 'exempt = []\n' if not exemptions else ''.join('[[exempt]]\nrs = '+json.dumps(row['rs'])+'\nreason = '+json.dumps(row['reason'])+'\n\n' for row in exemptions)
    (scope/'exempt.toml').write_text(text)
    print(f'MEXC scope: {len(exemptions)} existing venue exemptions; foreign crates are not indexed', flush=True)
    subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'run', '--manifest-path', str(sdk/'Cargo.toml'), '-p', 'safe-cex-parity', '--bin', 'parity-check', '--locked', '--jobs', '2', '--', '--workspace', str(scope), '--manifest', str(sdk/'parity/manifest/exchanges/mexc'), '--exempt', str(scope/'exempt.toml')], check=True)
