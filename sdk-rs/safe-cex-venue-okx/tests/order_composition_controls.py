import os
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-okx/src/order_composition.rs'
original = source.read_bytes()
log_dir = root / 'sdk-rs/target/okx-order-composition-controls'
log_dir.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-okx', '--jobs', '2', '--test', 'order_composition', '--', '--nocapture', '--test-threads=1']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
mutations = [
 ('single', [('(v(&h).await_value)(p).await\n}', '(v(&h).await_value)(p).await?;\n    Ok((v(&h).array)(vec![]))\n}')], 'SINGLE native result count'),
 ('place', [('(v(&ops).intrinsic)("subtract", vec![a, b])','(v(&ops).intrinsic)("subtract", vec![b, a])')], 'PLACE native algorithm priority order'),
 ('trigger', [('(v(h).capture)(if take {\n        "OrderType.TakeProfit"', '(v(h).capture)(if take {\n        "OrderType.StopLoss"')], 'TRIGGER native buy below reference type'),
 ('algo', [('(v(h).set)(&req, "triggerPx", stringify(h, trigger)?)?', '(v(h).set)(&req, "triggerPx", stringify(h, price.clone())?)?')], 'ALGO native adjusted trigger price'),
 ('create', [('let max = method(h, max, "multipliedBy", || Ok(vec![step]))?', 'let max = method(h, max, "multipliedBy", || Ok(vec![(v(h).number)(1.)]))?')], 'CREATE native decimal split widths'),
 ('orders', [('("filled", filled_base)', '("filled", remaining.clone())'), ('("remaining", remaining)', '("remaining", filled_base)')], 'ORDERS native converted remaining'),
 ('positions', [('("notional", notional)', '("notional", base.clone())'), ('("contracts", base)', '("contracts", notional)')], 'POSITIONS native base contracts'),
 ('fills', [('string(h, || (v(h).get)(&row, "tradeId"))?', 'string(h, || (v(h).get)(&row, "billId"))?')], 'FILLS native newest first'),
]
try:
    patched = original.decode()
    for name, edits, assertion in mutations:
        for old, new in edits:
            if patched.count(old) != 1:
                raise RuntimeError(f'{name}: expected one production site {old!r}')
            patched = patched.replace(old, new)
    source.write_text(patched)
    broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (log_dir / 'broken.log').write_text(broken.stdout)
    if broken.returncode != 101 or 'Finished `test`' not in broken.stdout or '0 passed; 8 failed' not in broken.stdout:
        raise RuntimeError(f'controls did not compile then fail: {broken.returncode}\n{broken.stdout}')
    for name, _, assertion in mutations:
        if assertion not in broken.stdout:
            raise RuntimeError(f'{name}: missing native assertion {assertion}\n{broken.stdout}')
    print('Eight compiled production controls: exit 101; all eight named native assertions reached', flush=True)
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(log_dir / 'restored.log').write_text(restored.stdout)
if restored.returncode != 0 or '8 passed; 0 failed' not in restored.stdout:
    raise RuntimeError(f'restored target failed: {restored.returncode}\n{restored.stdout}')
assert source.read_bytes() == original
print('Restored exact source bytes: exit 0; eight tests passed', flush=True)
