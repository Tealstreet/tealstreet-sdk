"""Paradex-only structural gates. Python >=3.11 (stdlib tomllib), Node and Cargo."""
import collections
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
SDK = ROOT / 'sdk-rs'
CARGO = os.environ.get('CARGO', 'cargo')
MANIFESTS = SDK / 'parity/manifest/exchanges/paradex'


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def read(path):
    with path.open('rb') as source:
        return tomllib.load(source)


extracted = json.loads(run('node', str(CRATE / 'scripts/extract_scope.cjs')))
for source in extracted:
    manifest = read(MANIFESTS / (Path(source['ts']).name + '.toml'))
    actual = {(r['ts'], r['kind']) for r in source['symbols']}
    expected = {(r['ts'], r['kind']) for r in manifest['symbol']}
    assert actual == expected, (source['ts'], actual - expected, expected - actual)
print(f'Paradex actual extractFile: {len(extracted)} files; symbol identity drift 0. No package typecheck claimed.', flush=True)

counts = collections.Counter(r['status'] for path in MANIFESTS.glob('*.toml') for r in read(path)['symbol'])
print(f'Paradex tomllib: total {sum(counts.values())}; {dict(counts)}', flush=True)
exempt = read(SDK / 'parity/exempt.toml')['exempt']
print(f'Global exempt tomllib: BLOCKS {len(exempt)} DISTINCT {len({r["rs"] for r in exempt})}', flush=True)
print(f'Paradex exempt blocks: {sum(r["rs"].startswith(CRATE.name + "/") for r in exempt)}', flush=True)

# The checker has no crate flag. Narrow its workspace member inventory, preserving
# the real venue manifest and source bytes, including the signing adapter row.
with tempfile.TemporaryDirectory(prefix='.parity-scope-', dir=CRATE) as directory:
    scope = Path(directory)
    config, replacements = re.subn(r'(?ms)^members = \[.*?^\]', f'members = ["{CRATE.name}"]', (SDK / 'Cargo.toml').read_text(), count=1)
    assert replacements == 1
    (scope / 'Cargo.toml').write_text(config)
    (scope / CRATE.name).symlink_to(CRATE, target_is_directory=True)
    (scope / 'manifests').mkdir()
    for manifest in sorted(MANIFESTS.glob('*.toml')):
        text = manifest.read_text()
        if manifest.name == 'paradex.sdk.ts.toml':
            def venue_targets(match):
                values = json.loads(match[2])
                kept = [value for value in values if value.startswith(CRATE.name + '/')]
                return match[1] + ' = [ ' + ', '.join(json.dumps(v) for v in kept) + ' ]'
            text = re.sub(r'(?m)^(rs|rust_files) = (\[.*\])$', venue_targets, text)
        (scope / 'manifests' / manifest.name).write_text(text)
    print('All eight venue manifests and the signing row included; shared crypto targets are outside this venue-only structure scope.', flush=True)
    (scope / 'exempt.toml').write_text('')
    print(run(CARGO, 'run', '--manifest-path', str(SDK / 'Cargo.toml'), '-p', 'safe-cex-parity', '--bin', 'parity-check', '--jobs', '1', '--', '--workspace', str(scope), '--manifest', str(scope / 'manifests'), '--exempt', str(scope / 'exempt.toml')), end='', flush=True)

# Direct production edges of this venue only. No other owner's layer suite runs.
meta = json.loads(run(CARGO, 'metadata', '--manifest-path', str(SDK / 'Cargo.toml'), '--locked', '--no-deps', '--format-version', '1'))
policy = json.loads((SDK / 'layers.json').read_text())
classes = {p: (i, layer['name']) for i, layer in enumerate(policy['layers']) for p in layer['packages']}
package = next(p for p in meta['packages'] if p['name'] == CRATE.name)
edges = [d for d in package['dependencies'] if d.get('path')]
for dep in edges:
    assert dep['name'] not in policy['privateDerivations']
    source, layer = classes[CRATE.name]
    target, _ = classes[dep['name']]
    assert source > target
    assert source - target == 1 or any(g['fromLayer'] == layer and g['to'] == dep['name'] for g in policy['layerGrants']) or any(e['from'] == CRATE.name and e['to'] == dep['name'] for e in policy['exceptions'])
    member = next(p for p in meta['packages'] if p['name'] == dep['name'])
    assert Path(dep['path']).resolve() == Path(member['manifest_path']).parent.resolve()
print(f'Paradex direct layer audit: {len(edges)} edges validated.', flush=True)
