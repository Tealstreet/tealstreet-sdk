"""Serial mutation controls; each test invokes Node once for the entire corpus."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
           '-p', 'safe-cex-venue-orderly', '--test', 'wire_parity', '--jobs', '1', '--locked']
def run():
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
def replace(source, old, new):
    assert old in source, old
    return source.replace(old, new, 1)
def in_record(source, name, change):
    start = source.index('struct ' + name + 'Wire {')
    end = source.index('\n}', start)
    return source[:start] + change(source[start:end]) + source[end:]
def swap(source, name, a, b):
    def change(block):
        for field, other in [(a, b), (b, a)]:
            block = replace(block, f'    {field}:', f'    #[serde(rename = "{other}")]\n    {field}:')
        return block
    return in_record(source, name, change)
def swap_side_variants(source):
    # Both codec directions change together, preserving a misleading roundtrip.
    old = 'pub enum EditOrderSide {\n    #[serde(rename = "BUY")]\n    Buy,\n    #[serde(rename = "SELL")]\n    Sell,\n}'
    new = 'pub enum EditOrderSide {\n    #[serde(rename = "SELL")]\n    Buy,\n    #[serde(rename = "BUY")]\n    Sell,\n}'
    assert source.count(old) == 1
    source = replace(source, old, new)
    old = 'enum EditOrderSideWire {\n    Buy,\n    Sell,\n}'
    new = 'enum EditOrderSideWire {\n    #[serde(rename = "SELL")]\n    Buy,\n    #[serde(rename = "BUY")]\n    Sell,\n}'
    assert source.count(old) == 1
    return replace(source, old, new)

controls = []
for name, a, b, assertion in [
    ('AccountResponse', 'taker_fee_rate', 'maker_fee_rate', 'wire/ACCOUNT_RESPONSE/full/native/taker_fee_rate/ingress0'),
    ('BalanceHolding', 'holding', 'frozen', 'wire/BALANCE_RESPONSE/full/native/holding/0/holding/ingress0'),
    ('PlaceOrderData', 'order_price', 'order_quantity', 'wire/PLACE_ORDER_RESPONSE/full/native/data/order_price/ingress0'),
    ('EditOrderRequest', 'order_price', 'order_quantity', 'wire/EDIT_ORDER_REQUEST/full/native/order_price/ingress0'),
    ('EditOrderResponse', 'timestamp', 'code', 'wire/EDIT_ORDER_RESPONSE/full/native/timestamp/ingress0'),
]:
    controls.append(('src/wire.rs', lambda s, n=name, a=a, b=b: swap(s, n, a, b), assertion))
controls.extend([
    ('src/wire.rs', lambda s: in_record(s, 'AccountResponse', lambda b: replace(b,
        '    maintenance_cancel_orders:', '    #[serde(skip_serializing)]\n    maintenance_cancel_orders:')),
        'wire/ACCOUNT_RESPONSE/full/serialization/ingress0'),
    ('src/wire.rs', lambda s: replace(s,
        'let value = String::deserialize(d)?;\n        AccountModeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))',
        'AccountModeWire::deserialize(d)'), 'literal/ACCOUNT_RESPONSE/account_mode/tagged'),
    ('src/wire.rs', lambda s: in_record(s, 'EditOrderData', lambda b: replace(b,
        ',\n        deserialize_with = "present"', '')), 'boundary/EDIT_ORDER_RESPONSE/null/data/status/raw'),
    ('src/wire.rs', lambda s: replace(s,
        'EditOrderDataWire::deserialize(safe_cex_types::strict_decode::object(decoder))',
        'EditOrderDataWire::deserialize(decoder)'), 'boundary/EDIT_ORDER_RESPONSE/nested-positional-array/data/raw'),
    ('src/wire.rs', lambda s: replace(s,
        'BalanceHoldingWire::deserialize(safe_cex_types::strict_decode::object(decoder))',
        'BalanceHoldingWire::deserialize(decoder)'), 'boundary/BALANCE_RESPONSE/array-item-positional/holding/0/raw'),
    ('tests/wire-oracle.cjs', lambda s: replace(s, 'return checker.getPropertiesOfType(type);',
        "return checker.getPropertiesOfType(type).filter(p => p.name !== 'requiredBase');"), 'inherited required-field census'),
    ('tests/wire_parity.rs', lambda s: replace(s, 'self.to_bits()', 'if *self == 0.0 { 0 } else { self.to_bits() }'),
        'wire/ACCOUNT_RESPONSE//max_leverage:-0/native/max_leverage/ingress0'),
])
controls.append(('src/wire.rs', swap_side_variants, 'literal/EDIT_ORDER_REQUEST/side/decoded-native'))
baseline = run()
assert baseline.returncode == 0, baseline.stdout
start = int(os.environ.get("ORDERLY_CONTROL_START", "0"))
assert 0 <= start < len(controls), "control start index outside corpus"
for relative, mutate, assertion in controls[start:]:
    file = crate / relative
    saved = file.read_bytes()
    try:
        file.write_text(mutate(saved.decode()))
        failed = run()
        assert failed.returncode == 101 and 'panicked at' in failed.stdout and assertion in failed.stdout, failed.stdout
        print('FAIL observed: ' + assertion, flush=True)
    finally:
        file.write_bytes(saved)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    print('PASS restored: ' + assertion, flush=True)
print(f'{len(controls) - start} mutations rejected; {len(controls) - start} restorations passed (start index {start})')
