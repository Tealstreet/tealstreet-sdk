#!/usr/bin/env python3
"""Temporary, shrinking inventory of legacy venue socket references.

Reuse the request ratchet's conservative Rust tokenizer and Git-base authority,
not its HTTP bootstrap exceptions. These are references, not resolved calls.
"""
import importlib.util
import json
import pathlib
import re
import subprocess

spec = importlib.util.spec_from_file_location(
    'socket_lexical_inventory', pathlib.Path(__file__).with_name('check-request-context.py'))
lex = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lex)
lex.METHODS = {'connect', 'connect_signed'}
ROOT = lex.ROOT
SITES = 'scripts/socket-legacy-sites.json'
FAMILIES = 'scripts/socket-legacy-families.json'


def family_manifest(text):
    value = json.loads(text, object_pairs_hook=lex.unique_object)
    if not isinstance(value, dict) or set(value) != {'temporaryFeature', 'sourceRoots'}:
        raise ValueError('invalid socket family inventory')
    if value['temporaryFeature'] != 'managed-sockets' or not isinstance(value['sourceRoots'], dict):
        raise ValueError('invalid temporary feature')
    for family, root in value['sourceRoots'].items():
        if (not re.fullmatch(r'[a-z0-9-]+', family)
                or not isinstance(root, str)
                or not re.fullmatch(r'venues/[a-z0-9-]+/src/(?:[a-z0-9_]+/)*', root)):
            raise ValueError('invalid socket family source root')
    if len(set(value['sourceRoots'].values())) != len(value['sourceRoots']):
        raise ValueError('duplicate socket family source root')
    return value


def previous(path, parser):
    # Use the same HEAD/parents/origin/immutable-event-base checks as HTTP.
    old_path, old_parser = lex.MANIFEST, lex.manifest
    try:
        lex.MANIFEST, lex.manifest = path, parser
        return lex.previous_manifests()
    finally:
        lex.MANIFEST, lex.manifest = old_path, old_parser


def check(current, allowed, families, old_sites=(), old_families=()):
    failures = []
    if current != allowed:
        failures.append('legacy socket references must exactly match inventory; prune removals')
    for label, baseline in old_sites:
        for path, counts in allowed.items():
            if not lex.subset(counts, baseline.get(path, {})):
                failures.append(f'{path}: legacy socket inventory grew against {label}')
    roots = families['sourceRoots']
    present = set()
    for path in current:
        matches = [(len(root), family) for family, root in roots.items() if path.startswith(root)]
        if not matches:
            failures.append(f'{path}: legacy socket family missing')
        else:
            present.add(max(matches)[1])
    if present != set(roots):
        failures.append('remaining socket families must exactly match source references')
    for label, baseline in old_families:
        old = baseline['sourceRoots']
        if any(old.get(family) != root for family, root in roots.items()):
            failures.append(f'legacy socket families grew or changed roots against {label}')
        # A removed nested family cannot silently reclassify as its parent.
        for path in current:
            owners = [(len(root), family) for family, root in old.items() if path.startswith(root)]
            if owners and max(owners)[1] not in roots:
                failures.append(f'{path}: still uses a retired socket family from {label}')
    return failures


def temporary_feature_present(source):
    # Conservative key-assignment scan, including quoted/escaped TOML keys,
    # dotted keys and inline tables. No extra TOML dependency on Python 3.9.
    key = r'''("(?:[^"\\]|\\.)*"|'[^']*'|[A-Za-z0-9_-]+)\s*='''
    for match in re.finditer(key, source):
        raw = match[1]
        if raw.startswith('"'):
            raw = re.sub(r'\\U([0-9a-fA-F]{8})', lambda m: chr(int(m[1], 16)), raw)
            name = json.loads(raw)
        else:
            name = raw[1:-1] if raw.startswith("'") else raw
        if name == 'managed-sockets':
            return True
    return False


def main():
    try:
        families = family_manifest((ROOT / FAMILIES).read_text())
        failures = check(
            lex.sites(), lex.manifest((ROOT / SITES).read_text()),
            families,
            previous(SITES, lex.manifest), previous(FAMILIES, family_manifest))
        if not families['sourceRoots'] and temporary_feature_present(
                (ROOT / 'engine/Cargo.toml').read_text()):
            failures.append('all socket families migrated: remove the temporary engine feature')
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        failures = [f'cannot verify legacy socket inventory: {type(error).__name__}']
    for failure in failures:
        print('BANNED: socket lifecycle: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
