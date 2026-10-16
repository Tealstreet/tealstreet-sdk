#!/usr/bin/env python3
"""Compiled Signer method body controls with native assertions."""
from pathlib import Path
import os
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-lighter-sdk', '--test', 'signing_runtime', '--jobs', '1', '--locked']
controls = [
    ('initialize', 'signing_runtime.rs', 'h.set_property(&receiver, "initialized", h.boolean(true))', 'h.set_property(&receiver, "initialized", h.boolean(false))', 'initialize_matches_source', 'SDK-SIGN state initialize 0'),
    ('guard', 'signing_runtime.rs', '|| !h.is_ready()?', '|| h.is_ready()?', 'ensure_initialized_matches_source', 'SDK-SIGN output ensureInitialized 0'),
    ('message', 'signing_runtime.rs', '("txHash", hashed), ("signature", signed)', '("txHash", signed), ("signature", hashed)', 'sign_message_matches_source', 'SDK-SIGN native hash signMessage 0'),
    ('create', 'signing_runtime.rs', 'HashKind::CreateOrder)', 'HashKind::CancelOrder)', 'sign_create_order_matches_source', 'SDK-SIGN calls signCreateOrder 0'),
    ('cancel', 'signing_runtime.rs', 'HashKind::CancelOrder)', 'HashKind::CreateOrder)', 'sign_cancel_order_matches_source', 'SDK-SIGN calls signCancelOrder 0'),
    ('transfer', 'signing_runtime.rs', 'HashKind::Transfer)', 'HashKind::Withdraw)', 'sign_transfer_matches_source', 'SDK-SIGN calls signTransfer 0'),
    ('withdraw', 'signing_runtime.rs', 'HashKind::Withdraw)', 'HashKind::Transfer)', 'sign_withdraw_matches_source', 'SDK-SIGN calls signWithdraw 0'),
    ('auth', 'signing_runtime.rs', '("token", signature),\n                ("message", message),\n                ("messageHash", tx_hash),', '("token", tx_hash),\n                ("message", message),\n                ("messageHash", signature),', 'create_auth_token_matches_source', 'SDK-SIGN auth native token createAuthToken 0'),
    ('verify', 'signing_runtime.rs', 'Failed to verify signature: ', 'Failed to verify message: ', 'verify_signature_matches_source', 'SDK-SIGN output verifySignature 2'),
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
        (sdk / 'target' / f'lighter-sdk-signing-runtime-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'{name}: {broken.stdout}'
        assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
        print(f'{name}: compiled 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'lighter-sdk-signing-runtime-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration {restored.stdout}'
    assert source.read_text() == original, 'production bytes restored'
    print(f'{name}: restored 0', flush=True)
