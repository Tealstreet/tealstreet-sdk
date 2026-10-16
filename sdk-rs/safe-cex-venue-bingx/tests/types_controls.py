#!/usr/bin/env python3
"""One production mutation per type row, plus a source-census control."""
import argparse
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
source = crate / 'src/types.rs'
oracle = crate / 'tests/types_oracle.cjs'
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
if not os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'):
    raise SystemExit('SAFE_CEX_TYPESCRIPT_PATH is required')
parser = argparse.ArgumentParser()
parser.add_argument('--only')
selected = parser.parse_args().only
completed = 0


def run():
    result = subprocess.run([cargo, 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
                             '-p', 'safe-cex-venue-bingx', '--jobs', '1', '--locked',
                             '--test', 'types', '--', '--nocapture'],
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return result.returncode, result.stdout


def replace_in(item, old, new):
    def apply(text):
        start = text.index(item)
        end = text.index('\n}', start)
        fragment = text[start:end]
        assert fragment.count(old) == 1, (item, old)
        return text[:start] + fragment.replace(old, new) + text[end:]
    return apply


def control(name, transform, assertion, path=source):
    global completed
    if selected and selected != name:
        return
    original = path.read_bytes()
    changed = transform(original.decode()).encode()
    assert changed != original
    try:
        path.write_bytes(changed)
        code, output = run()
        assert code != 0 and assertion in output and 'test result: FAILED' in output, (name, output)
    finally:
        path.write_bytes(original)
    code, output = run()
    assert code == 0 and '1 passed' in output, (name, output)
    completed += 1
    print(f'{name}: failed {assertion}; restored 1 passed', flush=True)


control('BINGX_BALANCE', replace_in('struct BingxBalanceWire', 'rename = "userId"', 'rename = "wrongUserId"'),
        'type_decode BINGX_BALANCE')
control('FETCH_BALANCE_RESPONSE', replace_in('struct FetchBalanceResponseWire', 'rename = "code"', 'rename = "wrongCode"'),
        'type_decode FETCH_BALANCE_RESPONSE')
control('BINGX_ORDER_STATUS', replace_in('pub enum BingxOrderStatus', 'rename = "CANCELED"', 'rename = "WRONG"'),
        'wire_shape BINGX_ORDER_STATUS')


def swap_leverage(text):
    start = text.index('struct FetchLeverageResponseDataWire')
    end = text.index('\n}', start)
    fragment = text[start:end].replace('rename = "longLeverage"', 'rename = "CONTROL"')
    fragment = fragment.replace('rename = "shortLeverage"', 'rename = "longLeverage"')
    fragment = fragment.replace('rename = "CONTROL"', 'rename = "shortLeverage"')
    return text[:start] + fragment + text[end:]


control('FETCH_LEVERAGE_RESPONSE', swap_leverage, 'native_fields FETCH_LEVERAGE_RESPONSE')
control('FETCH_MARGIN_MODE_RESPONSE', replace_in('pub enum BingxMarginType', 'rename = "CROSSED"', 'rename = "WRONG"'),
        'wire_shape FETCH_MARGIN_MODE_RESPONSE')
control('FETCH_HEDGE_MODE_RESPONSE', replace_in('struct FetchHedgeModeResponseWire', 'rename = "timeStamp"', 'rename = "wrongTimestamp"'),
        'type_decode FETCH_HEDGE_MODE_RESPONSE')
control('ORDER_TRADE_UPDATE', replace_in('struct OrderTradeUpdateDataWire', 'skip_serializing_if = "Option::is_none",', ''),
        'wire_shape ORDER_TRADE_UPDATE/absent:o/R')
control('FETCH_POSITIONS_RESPONSE', replace_in('struct FetchPositionsResponseDataWire', 'rename = "isolated"', 'rename = "wrongIsolated"'),
        'type_decode FETCH_POSITIONS_RESPONSE')
control('inherited_field_census',
        lambda text: text.replace('checker.getPropertiesOfType(type).map(symbol =>',
                                  "checker.getPropertiesOfType(type).filter(symbol => checker.typeToString(type) !== 'Child' || symbol.name !== 'inherited').map(symbol =>"),
        'inherited_field_census', oracle)
control('unresolved_heritage_refusal',
        lambda text: text.replace("assert(!(baseType.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)), 'unresolved heritage');",
                                  "assert(true, 'unresolved heritage');").replace("        schema(baseType, seen);", ""),
        'unresolved_heritage_refusal', oracle)
control('transitive_heritage_refusal',
        lambda text: text.replace('        schema(baseType, seen);', ''),
        'transitive_heritage_refusal', oracle)
assert completed > 0, f'no control named {selected!r}'
print(f'{completed} controls failed by name and passed after restoration', flush=True)
