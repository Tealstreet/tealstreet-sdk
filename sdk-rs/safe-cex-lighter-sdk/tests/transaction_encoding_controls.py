#!/usr/bin/env python3
"""Compiled encoding production mutations, each caught at a named oracle assertion."""
from pathlib import Path
import os
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-lighter-sdk', '--test', 'transaction_encoding', '--jobs', '1', '--locked']
hashes = 'transaction_messages_match_maintained_typescript'
amounts = 'amount_utilities_match_maintained_typescript'
controls = [
    ('create-order', 'transaction_encoding.rs', 'output.extend(i64_field(&tx.price)?);', 'output.extend(i64_field(&tx.quantity)?);', hashes, 'SDK-ENC native hashCreateOrderTx 0'),
    ('cancel-order', 'transaction_encoding.rs', 'append_identifier(&mut output, &tx.order_id);', 'append_identifier(&mut output, &tx.market_id);', hashes, 'SDK-ENC native hashCancelOrderTx 0'),
    ('transfer', 'transaction_encoding.rs', 'append_identifier(&mut output, &tx.recipient);', 'append_identifier(&mut output, &tx.token_address);', hashes, 'SDK-ENC native hashTransferTx 0'),
    ('withdraw', 'transaction_encoding.rs', 'tx.fast_withdraw.unwrap_or(false)', '!tx.fast_withdraw.unwrap_or(false)', hashes, 'SDK-ENC native hashWithdrawTx 0'),
    ('auth', 'transaction_encoding.rs', 'let mut output = u32_field(account_index)?;\n    output.extend(u32_field(api_key_index)?);', 'let mut output = u32_field(api_key_index)?;\n    output.extend(u32_field(account_index)?);', hashes, 'SDK-ENC native hashAuthToken 0'),
    ('bigint-hex', 'amount_encoding.rs', 'bytes * 2.0', 'bytes * 3.0', amounts, 'SDK-ENC native bigIntToHex 1'),
    ('format', 'amount_encoding.rs', 'let fractional = amount % &divisor;', 'let fractional = amount / &divisor;', amounts, 'SDK-ENC native formatAmount 0'),
    ('parse', 'amount_encoding.rs', 'runtime.pad_zeros(fraction, decimals, false)?', 'runtime.pad_zeros(fraction, decimals, true)?', amounts, 'SDK-ENC native parseAmount 1'),
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
        (sdk / 'target' / f'lighter-sdk-encoding-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'{name}: {broken.stdout}'
        assert 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
        print(f'{name}: compiled 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command + [test, '--', '--exact'], env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'lighter-sdk-encoding-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration {restored.stdout}'
    assert source.read_text() == original, 'production bytes restored'
    print(f'{name}: restored 0', flush=True)
