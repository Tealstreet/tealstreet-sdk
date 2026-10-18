"""String swaps pin native identity; the numeric swap is intercepted by the output guard."""
from pathlib import Path
import os
import subprocess
import sys
import re

path = Path('sdk-rs/safe-cex-venue-woo/src/types.rs')
original = path.read_text()
call = 'AccountResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))'
cases = [
    ('decoded-string-swap', original.replace(call, call + '.map(|mut value| { std::mem::swap(&mut value.account, &mut value.alias); value })'), 'AccountResponse.account native field identity'),
    ('symmetric-wire-rename', original.replace('rename = "accountMode"', 'rename = "__swap__"').replace('rename = "positionMode"', 'rename = "accountMode"').replace('rename = "__swap__"', 'rename = "positionMode"'), 'AccountResponse.account_mode native field identity'),
    ('decoded-number-swap', original.replace(call, call + '.map(|mut value| { std::mem::swap(&mut value.total_collateral, &mut value.free_collateral); value })'), 'numeric ingress changed Number bits at $.totalCollateral'),
]
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
for name, mutation, assertion in cases:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    assert mutation != original, name
    try:
        path.write_text(mutation)
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'field_identity'], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-field-identity-control-' + name + '.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)

for module, wire, fields, binary, target, assertion in [
    ('v3_public', 'WooV3FuturesRow', ('index_price', 'mark_price'), 'v3_field_identity', 'public_native_field_identity', 'index_price native field identity'),
    ('v3_contracts', 'WooV3PlaceOrderRequest', ('reduce_only', 'post_only_adjusted'), 'v3_field_identity', 'request_flags_and_nested_meta_native_identity', 'reduce_only native field identity'),
    ('v3_contracts', 'WooV3MigrationRoute', ('legacy_method', 'v3_method'), 'contract_field_identity', 'contract_native_field_identity', 'WooV3MigrationRoute.legacy_method native field identity'),
    ('v3_contracts', 'WooV3Balance', ('holding', 'frozen'), 'contract_field_identity', 'contract_native_field_identity', 'WooV3Balance.holding native field identity'),
]:
    name = wire + '-decoded-swap'
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    path = Path('sdk-rs/safe-cex-venue-woo/src/' + module + '.rs')
    original = path.read_text()
    matches = re.findall(wire + r'Wire::deserialize\(safe_cex_types::strict_decode::object\(\s*decoder,?\s*\)\)', original)
    assert len(matches) == 1, name
    call = matches[0]
    assert original.count(call) == 1, name
    mutation = original.replace(call, call + '.map(|mut value| { std::mem::swap(&mut value.' + fields[0] + ', &mut value.' + fields[1] + '); value })')
    try:
        path.write_text(mutation)
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', binary, target], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-field-identity-control-' + name + '.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)

for name, module, before, after, binary, assertion in [
    ('limit-id-symbol-swap', 'orders', 'id: text(order_id)?,\n        parent_id: None,', 'id: symbol.clone(),\n        parent_id: None,', 'orders', 'Order.id native field identity'),
]:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    path = Path('sdk-rs/safe-cex-venue-woo/src/' + module + '.rs')
    original = path.read_text()
    assert original.count(before) == 1, name
    mutation = original.replace(before, after).replace('        symbol,\n        r#type:', '        symbol: text(order_id)?,\n        r#type:', 1)
    try:
        path.write_text(mutation)
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', binary, 'legacy_mappers_match_source_and_pin_whole_record_refusals'], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-field-identity-control-' + name + '.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)

for name, module, first, second, binary, assertion in [
    ('history-price-average-swap', 'order_history', 'to_optional_number(row.get("price"))?', 'to_optional_number(row.get("average_executed_price"))?', 'order_history', 'HistoricalOrder.price native field identity'),
    ('journal-price-qty-swap', 'journal', '            price,\n            qty: order.filled,', '            price: order.filled,\n            qty: price,', 'journal', 'Execution.price native field identity'),
]:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    path = Path('sdk-rs/safe-cex-venue-woo/src/' + module + '.rs')
    original = path.read_text()
    assert original.count(first) == 1, name
    if module == 'order_history':
        assert original.count(second) == 1, name
        mutation = original.replace(first, '__FIELD_SWAP__').replace(second, first).replace('__FIELD_SWAP__', second)
    else:
        mutation = original.replace(first, second)
    try:
        path.write_text(mutation)
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', binary], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-field-identity-control-' + name + '.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)
