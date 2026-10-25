#!/usr/bin/env python3
"""Lint/test engine feature closures that workspace feature unification omits."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
ENGINE = 'safe-cex-engine'


def modes():
    families = json.loads(subprocess.check_output(
        ['node', '--input-type=module', '-e',
         "import('./hosts/wasm/js/venues.mjs').then(({venueFamilies}) => console.log(JSON.stringify(venueFamilies)))"],
        cwd=ROOT, text=True,
    ))
    if not families:
        raise RuntimeError('expected at least one venue family')
    closures = {}
    grouped = {
        (): ['engine baseline'],
        ('order-locks', 'test-support'): ['explicit CI control'],
        # metadata-freshness OFF control: standalone Bitget with test support.
        ('order-locks', 'polling-control', 'test-support'): ['metadata freshness OFF control'],
        # dispatch-nonce OFF control keeps ordinary Plan/read signing enabled.
        # Seeded serial-plans OFF control: never add serial-plans here. Once a family
        # adopts, workspace tests unify it on, so this row alone proves the off build.
        ('http-plans', 'order-locks', 'test-support'): ['dispatch nonce OFF control', 'serial plans OFF control'],
    }
    grouped[('metadata-freshness', 'test-support')] = ['metadata-only CI control']
    grouped[('http-plans', 'metadata-freshness', 'order-locks', 'test-support')] = [
        'metadata Plan CI control']
    # No family enables serial Plans before adoption; keep them compiled and tested.
    grouped[('http-plans', 'order-locks', 'serial-plans', 'test-support')] = ['serial Plan control']
    # Condition waits imply serial-plans; no family enables them before adoption.
    grouped[('condition-waits', 'http-plans', 'order-locks', 'serial-plans', 'test-support')] = [
        'condition wait control']
    # No family links session-bearer yet; its tests run only here.
    grouped[('http-mutation-barriers', 'http-plans', 'session-bearer', 'test-support')] = [
        'session-bearer control']
    # Seeded resend spans both opt-ins; their isolated modes cannot execute it.
    grouped[('condition-waits', 'http-mutation-barriers', 'http-plans', 'order-locks',
             'serial-plans', 'session-bearer', 'test-support')] = ['serial Session resend control']
    # session-bearer OFF control: Session bindings are refused (session_bearer_off.rs).
    grouped.setdefault(('http-plans', 'test-support'), []).append('session-bearer OFF control')
    grouped[('dispatch-nonce', 'http-mutation-barriers', 'http-plans', 'order-locks', 'test-support')] = ['explicit nonce control']
    for family in families:
        tree = subprocess.check_output([
            'cargo', 'tree', '--locked', '--target', 'wasm32-unknown-unknown',
            '-p', 'safe-cex-wasm', '--no-default-features', '--features', family,
            '-e', 'normal,build', '--prefix', 'none', '--format', '{p}|{f}',
        ], cwd=ROOT, text=True)
        resolved = set()
        for line in tree.splitlines():
            if line.startswith(ENGINE + ' v'):
                resolved.add(tuple(sorted(set(line.split('|', 1)[1]
                                             .removesuffix(' (*)').split(',')) - {''})))
        if len(resolved) != 1:
            raise RuntimeError(f'{family}: expected one engine feature closure, got {resolved}')
        features = resolved.pop()
        closures[family] = features
        grouped.setdefault(features, []).append(family)
    if not any('http-plans' in flags for flags in closures.values()):
        raise RuntimeError('expected at least one Plan-enabled family closure')
    return closures, grouped


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true', help='Resolve and print modes without compiling')
    args = parser.parse_args()
    closures, grouped = modes()
    print(json.dumps({'family_features': closures,
                      'engine_feature_modes': [{'features': flags, 'families': names}
                                            for flags, names in sorted(grouped.items())]}, indent=2),
          flush=True)
    if args.list:
        return
    env = dict(os.environ)
    env['RUSTFLAGS'] = (env.get('RUSTFLAGS', '') + ' -Dwarnings').strip()
    started = time.monotonic()
    for features, families in sorted(grouped.items()):
        mode_started = time.monotonic()
        flags = ['--features', ','.join(features)] if features else []
        print(f'Engine feature mode: {features or "empty"}; families={families}', flush=True)
        for check in ['clippy', 'test']:
            command = ['cargo', check, '--locked', '-p', ENGINE, '--all-targets',
                       '--no-default-features', *flags]
            if check == 'clippy':
                command += ['--', '-D', 'warnings']
            subprocess.run(command, cwd=ROOT, env=env, check=True)
        print(f'Mode completed in {time.monotonic() - mode_started:.1f}s', flush=True)
    print(f'All {len(grouped)} engine modes completed in {time.monotonic() - started:.1f}s',
          flush=True)


if __name__ == '__main__':
    main()
