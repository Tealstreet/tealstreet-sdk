#!/usr/bin/env python3
"""Compile one missing string-admission guard at a time, then restore exact bytes.

Run serially, without another compiler or source review on this worktree.
"""
from pathlib import Path
import os
import re
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitrue/src/wire.rs'
original = source.read_bytes()
environment = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1')
cases = [
    ('BitrueOrderType', 'bitrue_order_type_accepts_only_exact_string_literals', 'LIMIT'),
    ('FetchOrdersResponseDataItemSide', 'bitrue_order_side_accepts_only_exact_string_literals', 'BUY'),
]


def run(test=None):
    command = ['cargo', '+1.99.0', 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
               '--locked', '-p', 'safe-cex-venue-bitrue', '--test',
               'accepted_enum_shapes', '--jobs', '1']
    if test:
        command += [test, '--', '--exact']
    result = subprocess.run(command, cwd=root, env=environment, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    return result


try:
    baseline = run()
    assert baseline.returncode == 0, 'restored baseline must pass before controls'
    for owner, test, tag in cases:
        text = original.decode()
        pattern = (r"(impl<'de> Deserialize<'de> for " + owner +
                   r" \{\n    fn deserialize<D: serde::Deserializer<'de>>\(deserializer: D\) -> Result<Self, D::Error> \{)"
                   r"\n.*?(\n    \}\n\})")
        matches = list(re.finditer(pattern, text, re.S))
        assert len(matches) == 1, (owner, 'one concrete decoder required')
        assert 'String::deserialize(deserializer)?' in matches[0].group(0)
        mutated, count = re.subn(pattern,
                                lambda match: match.group(1) + '\n        ' + owner +
                                'Wire::deserialize(deserializer)' + match.group(2),
                                text, count=1, flags=re.S)
        assert count == 1 and mutated != text
        source.write_text(mutated)
        try:
            result = run(test)
            marker = f'BITRUE STRING SHAPE {owner} {tag} owned tagged map'
            assert result.returncode != 0 and 'test result: FAILED.' in result.stdout
            assert marker in result.stdout, (owner, 'must compile and fail named tagged-shape assertion')
            print(f'{owner}: compiled missing-guard control failed its named tagged-shape assertion', flush=True)
        finally:
            source.write_bytes(original)
        assert run(test).returncode == 0, (owner, 'exact restoration must pass')
finally:
    source.write_bytes(original)

assert source.read_bytes() == original
assert run().returncode == 0
print('Two compiled guard-removal controls detected; exact source restored; both native tests pass.')
