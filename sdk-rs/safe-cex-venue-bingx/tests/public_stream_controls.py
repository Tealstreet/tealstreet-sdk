#!/usr/bin/env python3
"""One compiled mutation per new public-stream family, with byte restoration."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
source = crate / 'src/public_stream.rs'
original = source.read_text()
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'public_stream', '--jobs', '1', '--locked']
controls = [
    ('guard', 'number.is_finite() && number > 0.0', 'number.is_finite() && number >= 0.0',
     'positive_guard_preserves_number_and_error_identity', 'BINGX-GUARD positive finite 3'),
    ('diagnostics', '.is_some_and(|symbol| symbols.iter().any(|s| s == symbol))',
     '.is_some_and(|symbol| !symbols.iter().any(|s| s == symbol))',
     'public_stream_matches_actual_typescript', 'BINGX-PUBLIC native formatMarketContractsDiagnostic variant 0'),
    ('connection', '("dataType".into(), J::String("market.contracts".into()))',
     '("dataType".into(), J::String("market.wrong".into()))',
     'public_stream_matches_actual_typescript', 'BINGX-PUBLIC operation order onOpen variant 0'),
    ('frames', '("mark", "fairPrice")', '("mark", "indexPrice")',
     'public_stream_matches_actual_typescript', 'BINGX-PUBLIC native ticker bits 0'),
]
for name, before, after, test, assertion in controls:
    assert original.count(before) == 1, f'{name}: unique production operation required'
    selected = command + [test, '--', '--exact']
    try:
        source.write_text(original.replace(before, after))
        broken = subprocess.run(selected, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (sdk / 'target' / f'bingx-public-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'FINDING {name}: exit {broken.returncode}, missing named assertion\n{broken.stdout}'
        assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
        print(f'{name}: compiled mutation exit 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(selected, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'bingx-public-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
    print(f'{name}: restoration exit 0', flush=True)
assert source.read_text() == original, 'production bytes restored'
