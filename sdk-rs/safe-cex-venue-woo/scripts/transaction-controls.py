"""Compile production transaction mutations and require named native failures."""
from pathlib import Path
import os
import subprocess

source = Path('sdk-rs/safe-cex-venue-woo/src/transaction.rs')
original = source.read_text()
env = dict(os.environ, CARGO_BUILD_JOBS='1', SAFE_CEX_TYPESCRIPT_PATH=str(Path.cwd() / 'node_modules/typescript'))
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
command = [cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'transaction']
controls = [
    ('price-qty', '        price,\n        qty,', '        price: qty,\n        qty: price,', 'TRANSACTION native price'),
    ('order-fill', '        order_id,\n        fill_id,', '        order_id: fill_id,\n        fill_id: order_id,', 'TRANSACTION native orderId'),
    ('maker-nan', 'Some(n != 0.0 && !n.is_nan())', 'Some(n != 0.0)', 'TRANSACTION native maker'),
    ('nullish-truthy', 'result.is_some_and(|v| !v.is_null())', 'js_truthy(result)', 'TRANSACTION native orderId'),
    ('early-price', '    let qty = number(', '    if price <= 0.0 { return Ok(None); }\n    let qty = number(', 'TRANSACTION coercion error'),
    ('timestamp-units', 'timestamp_seconds: (timestamp_milliseconds / 1000.0).floor()', 'timestamp_seconds: timestamp_milliseconds.floor()', 'TRANSACTION native timestampSeconds'),
    ('final-null', '    Ok(result)', '    Ok(result.filter(|value| !value.is_null()))', 'TRANSACTION native commission'),
]
try:
    for name, before, after, assertion in controls:
        assert original.count(before) == 1, name
        source.write_text(original.replace(before, after))
        run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-transaction-control-' + name + '.log').write_text(run.stdout)
        assert run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout, (name, run.stdout)
        print(name, '101', assertion, flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
restored = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert restored.returncode == 0, restored.stdout
print('restored target passed', flush=True)
