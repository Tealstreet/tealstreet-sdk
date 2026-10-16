"""Read-only structural cause census; classifications do not authorize repairs."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import re
import tomllib


def records(text):
    result = []
    for line in text.splitlines():
        if not line.strip():
            continue
        row = re.fullmatch(r'status: (.+\.ts)::(.+) \(([^)]+)\): (.+)', line)
        record = {'line': line}
        if row:
            source, symbol, kind, message = row.groups()
            record['row'] = (source, symbol, kind)
            rename = re.fullmatch(r'Rust item path must be (.+): (\S+); a rename note must name this exact Rust target', message)
            provenance = re.fullmatch(r'Rust source file is not declared for \S+: (\S+); declare the exact crate/file in \[file\]\.rust_files', message)
            if rename:
                record.update(category='rename', expected=rename[1], target=rename[2])
            elif provenance:
                record.update(category='provenance', target=provenance[1])
            elif message.startswith('rs does not resolve: '):
                record.update(category='unresolved', target=message.removeprefix('rs does not resolve: '))
            elif message == 'nonempty rs requires layer':
                record.update(category='layer')
        else:
            match = re.fullmatch(r'invention: (\S+): public item has no manifest rs or exemption', line)
            if match:
                record.update(category='unmapped', target=match[1])
            elif re.fullmatch(r'invention: \S+: exemption does not name a public Rust item', line):
                record.update(category='stale-exemption')
            elif re.fullmatch(r'status: \S+: declared Rust source file has no ported or divergent row mapping: \S+; remove unused \[file\]\.rust_files declaration', line):
                record.update(category='unused-file')
        if 'category' not in record:
            raise ValueError(f'Unclassified canonical diagnostic: {line}')
        result.append(record)
    if not result:
        raise ValueError('Expected a nonempty failing checker log')
    return result


def legacy_candidates(target):
    parts = target.split('::')
    crate = parts[0].replace('_', '-')
    for split in range(1, len(parts)):
        module = '/'.join(parts[1:split]) or 'lib'
        item = '::'.join(parts[split:])
        for file in (f'{crate}/src/{module}.rs', f'{crate}/src/{module}/mod.rs'):
            yield f'{file}::{item}'


def analyze(diagnostics, traits, manifests):
    unresolved = {r['target'] for r in diagnostics if r['category'] == 'unresolved'}
    legacy = {target for target in unresolved if '/' not in target}
    unmapped = {r['target'] for r in diagnostics if r['category'] == 'unmapped'}
    counterparts = {candidate for target in legacy for candidate in legacy_candidates(target) if candidate in unmapped}
    groups = defaultdict(Counter)
    details = []
    for record in diagnostics:
        category = record['category']
        target = record.get('target', '')
        cause = 'unassigned'
        unit = category
        if category == 'unmapped' and target in traits:
            cause = 'unclaimed-trait-api'
            unit = target.rsplit('::', 1)[0] if traits[target] == 'trait-member' else target
        elif target in legacy or category == 'unmapped' and target in counterparts:
            cause = 'legacy-crate-mapping'
            unit = target
        elif category == 'layer':
            row = manifests[record['row']]
            targets = [row['rs']] if isinstance(row['rs'], str) else row['rs']
            if targets and all(item in legacy for item in targets):
                cause = 'legacy-crate-mapping'
                unit = ' | '.join(targets)
        elif category == 'rename' and target not in unresolved and record['expected'].split('::')[-1] == target.split('::')[-1]:
            cause = 'receiver-namespace-relocation'
            unit = record['row'][0]
        elif category == 'provenance' and target not in unresolved:
            cause = 'missing-source-file-declaration'
            unit = record['row'][0] + ' -> ' + target.split('::')[0]
        groups[cause][unit] += 1
        details.append({**record, 'cause': cause, 'unit': unit})
    return {
        'diagnostics': len(diagnostics),
        'causes': {cause: {'diagnostics': sum(units.values()), 'groups': len(units), 'units': dict(sorted(units.items()))} for cause, units in sorted(groups.items())},
        'legacy_targets': len(legacy),
        'legacy_unmapped_candidates': len(counterparts),
        'unassigned_classes': dict(Counter(r['category'] for r in details if r['cause'] == 'unassigned')),
        'details': details,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--diagnostics', type=Path, required=True)
    parser.add_argument('--traits', type=Path, required=True)
    parser.add_argument('--manifests', type=Path, default=Path('sdk-rs/parity/manifest'))
    args = parser.parse_args()
    manifests = {}
    for file in sorted(args.manifests.rglob('*.toml')):
        data = tomllib.loads(file.read_text())
        for row in data.get('symbol', []):
            key = (data['file']['ts'], row['ts'], row['kind'])
            if key in manifests:
                raise ValueError(f'Duplicate source row: {key}')
            manifests[key] = row
    traits = {}
    for line in args.traits.read_text().splitlines():
        kind, target = line.split('\t')
        if kind not in {'trait', 'trait-member'} or target in traits:
            raise ValueError(f'Invalid trait inventory entry: {line}')
        traits[target] = kind
    result = analyze(records(args.diagnostics.read_text()), traits, manifests)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
