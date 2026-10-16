#!/usr/bin/env python3
"""Real parity index over the isolated SDK crate and its mapped source rows."""
from pathlib import Path
import json
import subprocess
import tempfile
import tomllib

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
build = subprocess.run([cargo, 'build', '--manifest-path', str(sdk / 'Cargo.toml'),
                       '-p', 'safe-cex-parity', '--bin', 'parity-check', '--jobs', '1',
                       '--locked', '--message-format=json'], check=True, text=True,
                       stdout=subprocess.PIPE)
artifacts = [json.loads(line) for line in build.stdout.splitlines()]
executables = [r['executable'] for r in artifacts
               if r.get('reason') == 'compiler-artifact'
               and r.get('target', {}).get('name') == 'parity-check' and r.get('executable')]
assert len(executables) == 1, 'exactly one real parity-check executable'
with tempfile.TemporaryDirectory(prefix='lighter-sdk-parity-', dir=sdk / 'target') as d:
    scope = Path(d)
    (scope / crate.name).symlink_to(crate, target_is_directory=True)
    (scope / 'Cargo.toml').write_text(
        '[workspace]\nmembers = ["safe-cex-lighter-sdk"]\n[workspace.dependencies]\n'
        f'safe-cex-crypto = {{ path = {json.dumps(str(sdk / "safe-cex-crypto"))} }}\n'
        f'safe-cex-num = {{ path = {json.dumps(str(sdk / "safe-cex-num"))} }}\n'
        'serde_json = "1"\n')
    (scope / 'manifest').mkdir()
    for path in (sdk / 'parity/manifest/lighter-sdk').rglob('*.toml'):
        m = tomllib.loads(path.read_text())
        selected = []
        for row in m.get('symbol', []):
            targets = row['rs'] if isinstance(row['rs'], list) else [row['rs']]
            if any(t.startswith(crate.name + '/') for t in targets):
                assert all(t.startswith(crate.name + '/') for t in targets), 'mixed-scope row'
                selected.append(row)
        if not selected:
            continue
        text = '[file]\nts = ' + json.dumps(m['file']['ts']) + '\nrust_files = ' + json.dumps(
            sorted({t.split('::')[0] for r in selected
                    for t in (r['rs'] if isinstance(r['rs'], list) else [r['rs']])})) + '\n'
        for row in selected:
            text += '\n[[symbol]]\n' + ''.join(
                f'{key} = {json.dumps(value, ensure_ascii=False)}\n' for key, value in row.items())
        (scope / 'manifest' / path.name).write_text(text)
    exemptions = tomllib.loads((sdk / 'parity/exempt.toml').read_text())['exempt']
    (scope / 'exempt.toml').write_text(''.join(
        f'[[exempt]]\nrs = {json.dumps(r["rs"])}\nreason = {json.dumps(r["reason"])}\n\n'
        for r in exemptions if r['rs'].startswith(crate.name + '/')))
    subprocess.run([executables[0], '--workspace', str(scope), '--manifest',
                    str(scope / 'manifest'), '--exempt', str(scope / 'exempt.toml')], check=True)
