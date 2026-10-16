#!/usr/bin/env python3
"""Sequential destructive controls, restricted to this worktree; always restore."""
import argparse
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
root = sdk.parent
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
env = dict(os.environ)
if not env.get('SAFE_CEX_TYPESCRIPT_PATH'):
    raise SystemExit('SAFE_CEX_TYPESCRIPT_PATH is required')
source = crate / 'src/constants.rs'
ts_source = root / 'packages/safe-cex/src/exchanges/bingx/bingx.types.ts'
parser = argparse.ArgumentParser()
parser.add_argument('--only', help='run one named control instead of all controls')
selected = parser.parse_args().only
completed = 0


def run(test):
    result = subprocess.run([
        cargo, 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
        '-p', 'safe-cex-venue-bingx', '--jobs', '1', '--locked',
        '--test', 'constants', test, '--', '--exact',
    ], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return result.returncode, result.stdout


def control(label, path, transform, assertion, test='constants_match_actual_typescript'):
    global completed
    if selected is not None and selected != label:
        return
    original = path.read_bytes()
    changed = transform(original.decode()).encode()
    assert changed != original, label
    try:
        path.write_bytes(changed)
        code, output = run(test)
        assert code != 0 and assertion in output and 'test result: FAILED' in output, (label, output)
    finally:
        path.write_bytes(original)
    code, output = run(test)
    assert code == 0 and '1 passed' in output, (label, output)
    completed += 1
    print(f'{label}: failed {assertion}; restored 1 passed', flush=True)


def replace_once(old, new):
    def apply(text):
        assert text.count(old) == 1, old
        return text.replace(old, new)
    return apply


control('RECV_WINDOW native number', source, replace_once('20_000.0', '20_001.0'), 'RECV_WINDOW native bits')
# Mutate each declaration separately. Even endpoints with duplicate URL values
# retain different keys, while inverse-collision order has a separate control.
for name in ['BASE_URL', 'BASE_WS_URL', 'ENDPOINTS', 'GRAFANA_ENDPOINTS_EVERY',
             'GRAFANA_ENDPOINTS_PERIODIC', 'ORDER_TYPE', 'ORDER_SIDE', 'POSITION_SIDE',
             'TIME_IN_FORCE', 'ORDER_STATUS', 'ORDER_TRIGGER_TYPE']:
    def mutate(text, name=name):
        start = text.index(f'pub const {name}:')
        end = text.index('\n];', start) + 3 if name not in ('ORDER_SIDE', 'POSITION_SIDE') else text.index(';', text.index(' =', start)) + 1
        declaration = text[start:end]
        if name.startswith('GRAFANA_'):
            declaration = declaration.replace('ENDPOINTS[', 'ENDPOINTS[1 + ', 1)
        else:
            quote = declaration.index('"')
            declaration = declaration[:quote + 1] + 'CONTROL_' + declaration[quote + 1:]
        return text[:start] + declaration + text[end:]
    control(f'{name} value/order', source, mutate, f'ordered constant {name}')

control('same-typed value swap', source,
        replace_once('[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]',
                     '[("BUY", OrderSide::Sell), ("SELL", OrderSide::Buy)]'),
        'ordered constant ORDER_SIDE')
control('inverse collision winner', source,
        replace_once('    ("TRIGGER_MARKET", OrderType::StopLoss),\n    ("STOP_MARKET", OrderType::StopLoss),',
                     '    ("STOP_MARKET", OrderType::StopLoss),\n    ("TRIGGER_MARKET", OrderType::StopLoss),'),
        'inverse order ORDER_TYPE', 'mapping_lookup_and_inverse_match_actual_typescript')
control('actual source execution', ts_source,
        replace_once("BALANCE: '/openApi/swap/v3/user/balance'", "BALANCE: '/control/balance'"),
        'ordered constant ENDPOINTS')
control('inherited identity observation', ts_source,
        lambda text: text + '\nObject.setPrototypeOf(ORDER_SIDE, null);\n',
        'inherited lookup ORDER_SIDE/', 'mapping_lookup_and_inverse_match_actual_typescript')
assert completed > 0, f'no control named {selected!r}'
print(f'{completed} controls failed by name and passed after restoration', flush=True)
