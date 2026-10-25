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
# Reviewed, single-use edits of an existing site's arguments: never a new site.
# Once every baseline carries the new digest (the edit landed), an entry is only
# reported for removal: its old digest is gone, so it can never be reused.
REPLACEMENTS = 'scripts/socket-legacy-replacements.json'


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


def replacement_manifest(text):
    """{path: [{"old": digest, "new": digest}, ...]}: each renames one occurrence."""
    value = json.loads(text, object_pairs_hook=lex.unique_object)
    if not isinstance(value, dict):
        raise ValueError('invalid socket replacement inventory')
    for path, entries in value.items():
        if (not re.fullmatch(r'venues/[A-Za-z0-9_-]+/src/.+\.rs', path)
                or '..' in path.split('/') or not isinstance(entries, list) or not entries):
            raise ValueError('invalid socket replacement path')
        for entry in entries:
            if (not isinstance(entry, dict) or set(entry) != {'old', 'new'}
                    or not all(isinstance(entry[key], str) and re.fullmatch(r'[a-f0-9]{64}', entry[key])
                               for key in ('old', 'new'))
                    or entry['old'] == entry['new']):
                raise ValueError('invalid socket replacement entry')
    return value


def replaced(counts, baseline, entries):
    """`counts` with each new digest renamed back to its old one, or None when an
    entry's old digest is absent at `baseline` or its new digest is unused."""
    effective = dict(counts)
    for entry in entries:
        if baseline.get(entry['old'], 0) <= 0 or effective.get(entry['new'], 0) <= 0:
            return None
        effective[entry['new']] -= 1
        if not effective[entry['new']]:
            del effective[entry['new']]
        effective[entry['old']] = effective.get(entry['old'], 0) + 1
    return effective


def previous(path, parser):
    # Use the same HEAD/parents/origin/immutable-event-base checks as HTTP.
    old_path, old_parser = lex.MANIFEST, lex.manifest
    try:
        lex.MANIFEST, lex.manifest = path, parser
        return lex.previous_manifests()
    finally:
        lex.MANIFEST, lex.manifest = old_path, old_parser


def check(current, allowed, families, old_sites=(), old_families=(), replacements=None,
          notices=None):
    failures = []
    replacements = replacements or {}
    used = set()
    landed = set()
    if current != allowed:
        failures.append('legacy socket references must exactly match inventory; prune removals')
    for label, baseline in old_sites:
        for path, counts in allowed.items():
            base = baseline.get(path, {})
            if lex.subset(counts, base):
                if any(base.get(entry['new'], 0) > 0 and base.get(entry['old'], 0) <= 0
                       for entry in replacements.get(path, [])):
                    landed.add(path)
                continue
            # An argument edit of a site that exists at this baseline, count unchanged.
            effective = replaced(counts, base, replacements.get(path, []))
            if effective is not None and lex.subset(effective, base):
                used.add(path)
                continue
            failures.append(f'{path}: legacy socket inventory grew against {label}')
    for path in replacements:
        if path in used:
            continue
        if path in landed and notices is not None:
            notices.append(f'{path}: legacy socket replacement landed; remove its entry')
        elif path not in landed:
            failures.append(f'{path}: stale or unusable legacy socket replacement')
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
    notices = []
    try:
        families = family_manifest((ROOT / FAMILIES).read_text())
        failures = check(
            lex.sites(), lex.manifest((ROOT / SITES).read_text()),
            families,
            previous(SITES, lex.manifest), previous(FAMILIES, family_manifest),
            replacement_manifest((ROOT / REPLACEMENTS).read_text()), notices)
        if not families['sourceRoots'] and temporary_feature_present(
                (ROOT / 'engine/Cargo.toml').read_text()):
            failures.append('all socket families migrated: remove the temporary engine feature')
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        failures = [f'cannot verify legacy socket inventory: {type(error).__name__}']
    for notice in notices:
        print('NOTICE: socket lifecycle: ' + notice)
    for failure in failures:
        print('BANNED: socket lifecycle: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
