#!/usr/bin/env python3
"""Compiled API read and signature body controls with native assertions."""
from pathlib import Path
import os
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-lighter-sdk', '--test', 'api_reads', '--jobs', '1', '--locked']
reads = 'source_read_bodies_keep_requests_results_and_eager_ownership'
controls = [
    ('referral-class-constructor', 'api_reads.rs', '        Self { client }', '        std::mem::forget(client.clone());\n        Self { client }', reads, 'SDK-READ constructor ownership'),
    ('referral-query', 'api_reads.rs', '/api/v1/referral/points', '/api/v1/referral/wrong', reads, 'SDK-READ request referral 0'),
    ('nonce', 'api_reads.rs', 'return h.bigint(&h.get(&payload, "nonce")?);', 'return h.bigint(&h.number(0.0));', reads, 'SDK-READ result nonce 6'),
    ('api-keys', 'api_reads.rs', 'h.promise_resolve(h.get(&data, "apiKeys")?).await', 'h.promise_resolve(h.get(&data, "wrong")?).await', reads, 'SDK-READ result keys 28'),
    ('transfer-fee', 'api_reads.rs', 'field("minAmount", min),\n                field("maxAmount", max),', 'field("minAmount", max),\n                field("maxAmount", min),', reads, 'SDK-READ result fee 32'),
    ('order-book', 'api_reads.rs', 'h.number(h.now()?)', 'h.number(h.now()? + 1.0)', reads, 'SDK-READ result orders 34'),
    ('signature-wire', 'signature_wire.rs', '.map(parse_pair)', '.map(|p| parse_pair(p).wrapping_add(1))', 'source_signature_wire_keeps_parseint_bytes_and_encoder_selection', 'SDK-READ signature result 2'),
]
selected = set(sys.argv[1:])
assert selected <= {c[0] for c in controls}, 'unknown family'
for name, file, before, after, test, assertion in controls:
    if selected and name not in selected:
        continue
    source = crate / 'src' / file
    original = source.read_text()
    assert original.count(before) == 1, f'{name}: unique production operation'
    try:
        source.write_text(original.replace(before, after))
        broken = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (sdk / 'target' / f'lighter-sdk-api-reads-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'{name}: {broken.stdout}'
        assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
        print(f'{name}: compiled 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'lighter-sdk-api-reads-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration {restored.stdout}'
    assert source.read_text() == original, 'production bytes restored'
    print(f'{name}: restored 0', flush=True)
