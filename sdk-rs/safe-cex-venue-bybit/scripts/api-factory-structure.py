"""Python >=3.11: check complete changed crates and dependencies with parity-check."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
BASE = 'd19d2874e0265c85602b9240d618612c65984c02'


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def read(path):
    return tomllib.loads(path.read_text())


def quote(value):
    return json.dumps(value, ensure_ascii=False)


def mapping(row):
    value = row['rs']
    return [value] if isinstance(value, str) and value else value or []


def manifest_text(manifest):
    lines = ['[file]'] if manifest['symbol'] else ['symbol = []', '', '[file]']
    for key, value in manifest['file'].items():
        lines.append(f'{key} = {quote(value)}')
    for row in manifest['symbol']:
        lines.extend(['', '[[symbol]]'])
        for key, value in row.items():
            lines.append(f'{key} = {quote(value)}')
    text = '\n'.join(lines) + '\n'
    assert tomllib.loads(text) == manifest, 'manifest serialization changed data'
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checker', type=Path, default=SDK / 'target/debug/parity-check')
    args = parser.parse_args()
    checker = args.checker.resolve(strict=True)
    head = git('rev-parse', 'HEAD')
    config = read(SDK / 'Cargo.toml')
    members = config['workspace']['members']
    assert all('*' not in member for member in members), 'explicit members required'
    crates = {member: read(SDK / member / 'Cargo.toml') for member in members}
    names = {data['package']['name']: member for member, data in crates.items()}
    assert len(names) == len(crates), 'duplicate workspace package names'
    by_path = {(SDK / member).resolve(): member for member in members}
    paths = set(git('diff', '--name-only', BASE, '--', 'sdk-rs').splitlines())
    paths.update(git('ls-files', '--others', '--exclude-standard', 'sdk-rs').splitlines())
    changed = set()
    for path in paths:
        parts = Path(path).parts
        if (len(parts) >= 4 and parts[0] == 'sdk-rs' and parts[2] == 'src'
                and path.endswith('.rs') and 'tests' not in parts[3:]
                and not parts[-1].endswith('_tests.rs') and parts[-1] != 'tests.rs'):
            assert parts[1] in crates, f'changed source outside workspace members: {path}'
            changed.add(parts[1])
    assert {'safe-cex-exchange', 'safe-cex-transport', 'safe-cex-venue-bybit'} <= changed
    selected = set()
    pending = list(changed)
    while pending:
        member = pending.pop()
        if member in selected:
            continue
        selected.add(member)
        crate = crates[member]
        for section in [crate, *crate.get('target', {}).values()]:
            for alias, dependency in section.get('dependencies', {}).items():
                base = SDK / member
                if isinstance(dependency, dict) and dependency.get('workspace'):
                    dependency = config['workspace']['dependencies'][alias]
                    base = SDK
                if isinstance(dependency, dict) and 'path' in dependency:
                    path = (base / dependency['path']).resolve(strict=True)
                    assert path in by_path, f'production path dependency outside workspace: {path}'
                    pending.append(by_path[path])
    packages = {crates[member]['package']['name'] for member in selected}

    def in_scope(target):
        return target.split('/', 1)[0] in packages

    projected = []
    retained = 0
    removed_targets = 0
    manifest_paths = sorted((SDK / 'parity/manifest').rglob('*.toml'))
    for path in manifest_paths:
        original = read(path)
        relevant = (any(in_scope(target) for row in original.get('symbol', []) for target in mapping(row))
                    or any(in_scope(file) for file in original['file'].get('rust_files', []))
                    or original['file']['ts'] == 'packages/safe-cex/src/exchanges/bybit/bybit.api.ts')
        if not relevant:
            continue
        rows = []
        for row in original.get('symbol', []):
            targets = mapping(row)
            kept = [target for target in targets if in_scope(target)]
            if targets and not kept:
                continue
            result = dict(row)
            if targets:
                result['rs'] = kept if isinstance(row['rs'], list) else kept[0]
                removed_targets += len(targets) - len(kept)
            assert all(result[key] == value for key, value in row.items() if key != 'rs')
            rows.append(result)
        file = dict(original['file'])
        if 'rust_files' in file:
            file['rust_files'] = [value for value in file['rust_files'] if in_scope(value)]
        projected.append((path.relative_to(SDK / 'parity/manifest'), {'file': file, 'symbol': rows}))
        retained += len(rows)
    assert projected and retained, 'empty structural scope'
    factory = [row for _, manifest in projected
               if manifest['file']['ts'] == 'packages/safe-cex/src/exchanges/bybit/bybit.api.ts'
               for row in manifest['symbol'] if row['ts'] == 'createAPI']
    assert len(factory) == 1, 'createAPI annotation must remain in scope'
    exemptions = [entry for entry in read(SDK / 'parity/exempt.toml')['exempt'] if in_scope(entry['rs'])]
    assert len({entry['rs'] for entry in exemptions}) == len(exemptions), 'duplicate scoped exemptions'
    inputs = [SDK / 'Cargo.toml', SDK / 'parity/exempt.toml', checker, *manifest_paths]
    for member in sorted(selected):
        inputs.extend([SDK / member / 'Cargo.toml', *sorted((SDK / member / 'src').rglob('*.rs'))])

    def fingerprint():
        return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}

    before = fingerprint()
    print(f'Base: {BASE}; measured HEAD: {head}', flush=True)
    print(f'Changed production crates: {json.dumps(sorted(changed))}', flush=True)
    print(f'Complete production dependency closure: {json.dumps(sorted(selected))}', flush=True)
    print(f'Projected source manifests: {len(projected)}; rows: {retained}; outside split targets: {removed_targets}; exact exemptions: {len(exemptions)}', flush=True)
    print(f'Existing checker SHA256: {before[str(checker)]}; no fresh-build/source-version claim.', flush=True)
    print(f'Annotations are preserved; createAPI status: {factory[0]["status"]}.', flush=True)
    print('Full-workspace structure is NOT PASSED by this scoped run. The separate d74a1c01f7 base measurement failed on webworker thread_local; no fix or exemption is claimed here.', flush=True)
    with tempfile.TemporaryDirectory(prefix='bybit-api-structure-') as directory:
        scope = Path(directory)
        text, replacements = re.subn(r'(?ms)^members = \[.*?^\]',
                                    'members = ' + quote(sorted(selected)),
                                    (SDK / 'Cargo.toml').read_text(), count=1)
        assert replacements == 1, 'expected exactly one workspace member list'
        expected = dict(config)
        expected['workspace'] = dict(config['workspace'], members=sorted(selected))
        assert tomllib.loads(text) == expected, 'workspace projection changed more than members'
        (scope / 'Cargo.toml').write_text(text)
        for member in members:
            (scope / member).symlink_to(SDK / member, target_is_directory=True)
        manifests = scope / 'manifests'
        manifests.mkdir()
        for relative, manifest in projected:
            path = manifests / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(manifest_text(manifest))
        exempt = scope / 'exempt.toml'
        exempt.write_text(''.join(f'[[exempt]]\nrs = {quote(row["rs"])}\nreason = {quote(row["reason"])}\n\n'
                                  for row in exemptions))
        assert read(exempt) == {'exempt': exemptions}, 'exemption projection changed data'
        command = [str(checker), '--workspace', str(scope), '--manifest', str(manifests), '--exempt', str(exempt)]
        print('Command: ' + json.dumps(command), flush=True)
        result = subprocess.run(command, cwd=ROOT)
    assert git('rev-parse', 'HEAD') == head, 'HEAD changed during structural check'
    assert fingerprint() == before, 'structural inputs changed during check'
    print(f'Scoped real checker exit: {result.returncode}; no full-workspace or behavioral approval claimed.', flush=True)
    return result.returncode


if __name__ == '__main__':
    raise SystemExit(main())
