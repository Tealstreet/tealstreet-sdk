#!/usr/bin/env python3
"""Inject consuming faults in shared Array only in this worktree; restore every byte.

This is a temporary shared production mutation, never a shared implementation edit.
Run serially with no other Cargo or source editing in this worktree.
"""
from pathlib import Path
import hashlib
import os
import re
import subprocess
import argparse

parser = argparse.ArgumentParser()
parser.add_argument("--leverage-only", action="store_true")
args = parser.parse_args()

crate = Path(__file__).resolve().parents[1]
shared = crate.parent / 'safe-cex-num/src/numeric_ingress.rs'
original = shared.read_bytes()
anchor = '''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
assert original.decode().count(anchor) == 1
empty = '''        let _ = seed;
        self.0.by_ref().for_each(drop);
        Ok(None)'''
subset = '''        let first = self.0.next();
        self.0.by_ref().for_each(drop);
        first.map(|value| seed.deserialize(JsNumberDeserializer(value.0))).transpose()'''
nested = '''        if self.0.len() == 2 && self.0.as_slice().iter().all(|value| matches!(value.0, Node::Lexeme(_) | Node::Float(_))) {
            let _ = seed;
            self.0.by_ref().for_each(drop);
            return Ok(None);
        }
''' + anchor
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--locked', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-kraken', '--jobs', '1']

def run(target):
    return subprocess.run(command + ['--test', target, 'leverage_' if args.leverage_only else 'consuming_'], cwd=crate.parent,
                          env=os.environ, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

for target in (['reads'] if args.leverage_only else ['utilities', 'reads']):
    baseline = run(target)
    assert baseline.returncode == 0, baseline.stdout

configurations = [('empty', empty, False), ('empty-lengths-bypassed', empty, True)] if args.leverage_only else [('empty', empty, False), ('subset', subset, False), ('empty-lengths-bypassed', empty, True), ('nested-empty', nested, False)]
for mode, replacement, bypass in configurations:
    targets = ['reads'] if args.leverage_only or mode == 'nested-empty' else ['utilities', 'reads']
    backups = {}
    try:
        if bypass:
            for target in targets:
                path = crate / f'tests/{target}.rs'
                backups[path] = path.read_bytes()
                text = backups[path].decode()
                if args.leverage_only:
                    start = text.index('fn leverage_settings_live_oracle()')
                    prefix, body = text[:start], text[start:]
                    body, n = re.subn(r'        cardinality::native_len\(\n.*?\n        \);\n', '', body, flags=re.S)
                    assert n == 2, ('leverage native length guard count', n)
                    path.write_text(prefix + body)
                    continue
                text, n = re.subn(r'    assert_eq!\(\n        values.len\(\),\n        \d+,\n        "KRAKEN-CONSUMING [^"]+ independent native length"\n    \);\n', '', text)
                assert n == 2, (target, 'independent length guard count', n)
                text, n = re.subn(r'    cardinality::native_len\(\n        values.len\(\),\n        &case\["result"\],\n        "KRAKEN-CONSUMING [^"]+ source/native length",\n    \);\n', '', text)
                assert n == 2, (target, 'source/native guard count', n)
                path.write_text(text)
        shared.write_text(original.decode().replace(anchor, replacement))
        for target in targets:
            result = run(target)
            if args.leverage_only:
                assertions = ['leverage native cache mutation trace']
            elif mode == 'nested-empty':
                assertions = ['KRAKEN-CONSUMING token nested native length']
            else:
                names = ['fills', 'income'] if target == 'utilities' else ['account', 'token']
                suffix = 'native index coverage' if bypass else 'independent native length'
                assertions = [f'KRAKEN-CONSUMING {name} {suffix}' for name in names]
            assert result.returncode == 101 and 'test result: FAILED' in result.stdout, (mode, target, result.stdout)
            for assertion in assertions:
                assert assertion in result.stdout, (mode, target, assertion, result.stdout)
                print(f'{mode}/{target}: compiled; exit 101; {assertion}', flush=True)
    finally:
        shared.write_bytes(original)
        for path, data in backups.items():
            path.write_bytes(data)
            assert path.read_bytes() == data
    assert shared.read_bytes() == original
    for target in targets:
        restored = run(target)
        assert restored.returncode == 0, (mode, target, restored.stdout)
        print(f'{mode}/{target}: restored pass; shared sha256 {hashlib.sha256(original).hexdigest()}', flush=True)
print('2 leverage consuming configurations; 2 named failures; 2 restored passing targets' if args.leverage_only else '4 consuming mutation configurations; 7 failing target runs; 13 named path failures; 7 restored passing target runs', flush=True)
