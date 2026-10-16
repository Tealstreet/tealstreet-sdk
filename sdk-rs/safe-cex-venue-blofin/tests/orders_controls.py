"""One row mutation and three production boundary controls, sequential whole-corpus fail/restore tests."""
import os
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/orders.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'orders', '--', '--nocapture']
MUTATIONS = {
    'BlofinExchange.mapOrders': ('let amount = multiply(amount_native, contract);', 'let amount = subtract(amount_native, contract);', 'orders_native_fields'),
}
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
rows = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.exchange.ts.toml').read_text())['symbol']
assert {r['ts'] for r in rows if r['status'] != 'pending' and any('/orders.rs::' in target for target in (r['rs'] if isinstance(r['rs'], list) else [r['rs']]))} == set(MUTATIONS), 'orders_row_mutation_census'
MUTATIONS.update({
    'clock_control': ('let side = side(order_side.as_deref())?;', 'if order_side.is_none() { now(); } let side = side(order_side.as_deref())?;', 'orders_refusal_clock_boundary'),
    'typed_control': ('Some("sell") => Ok(OrderSide::Sell),\n        _ => Err(representation()),', 'Some("sell") => Ok(OrderSide::Sell),\n        _ => Ok(OrderSide::Buy),', 'orders_typed_boundary'),
    'ingress_control': ('#[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]', '', 'orders_ingress_retained'),
})
original = SOURCE.read_text()
logs = ROOT / 'sdk-rs/target/blofin-orders-controls'
logs.mkdir(exist_ok=True)
for name, (old, new, assertion) in MUTATIONS.items():
    assert original.count(old) == 1, name + '_anchor'
    try:
        SOURCE.write_text(original.replace(old, new))
        result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / (name + '.fail.log')).write_text(result.stdout)
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print(f'{name}: FAIL {assertion} exit=101', flush=True)
    finally:
        SOURCE.write_text(original)
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.restored.log')).write_text(result.stdout)
    assert result.returncode == 0, result.stdout
    print(f'{name}: RESTORED exit=0', flush=True)
