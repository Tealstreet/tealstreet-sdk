import os
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-okx/src/websocket_account_events.rs'
original = source.read_bytes()
log_dir = root / 'sdk-rs/target/okx-websocket-account-controls'
log_dir.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-okx', '--jobs', '2', '--test', 'websocket_account_events', '--', '--nocapture', '--test-threads=1']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
mutations = [
    ('private/business state', [('(h.value.number)(60_000.)', '(h.value.number)(60_001.)')], 'CLASS native private dedupe window'),
    ('public state', [('("tickers", "this.handleTickerEvents")', '("tickers", "this.handleMarkPriceEvents")')], 'CLASS native handler identity and late member lookup'),
    ('order', [('"OrderStatus.Closed"','"OrderStatus.Canceled"')], 'ORDER native terminal status'),
    ('dedupe', [('"less-than",\n                vec![seen, cutoff.clone()],', '"less-or-equal",\n                vec![seen, cutoff.clone()],')], 'DEDUPE native retains exact cutoff'),
    ('position', [('define(&h, &balance, "used", used)?;', 'define(&h, &balance, "used", upnl.clone())?;'), ('define(&h, &balance, "upnl", upnl)?;', 'define(&h, &balance, "upnl", used)?;')], 'POSITION native rounded margin USD'),
    ('account', [('"free",\n        call(&h, "subtract", || Ok(vec![collateral.clone(), used]))?', '"total",\n        call(&h, "subtract", || Ok(vec![collateral.clone(), used]))?'), ('"total",\n        call(&h, "subtract", || Ok(vec![collateral, upnl]))?', '"free",\n        call(&h, "subtract", || Ok(vec![collateral, upnl]))?')], 'ACCOUNT native free after used'),
    ('markets', [('offset + 100.', 'offset + 99.'), ('offset += 100.', 'offset += 99.')], 'MARKETS native 100-topic batch widths'),
]
try:
    patched = original.decode()
    for name, edits, assertion in mutations:
        for old, new in edits:
            if patched.count(old) != 1:
                raise RuntimeError(f'{name}: expected exactly one production site {old!r}')
            patched = patched.replace(old, new)
    source.write_text(patched)
    broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (log_dir / 'broken.log').write_text(broken.stdout)
    if broken.returncode != 101 or 'Finished `test`' not in broken.stdout or '0 passed; 7 failed' not in broken.stdout:
        raise RuntimeError(f'controls did not compile then fail: {broken.returncode}\n{broken.stdout}')
    for name, _, assertion in mutations:
        if assertion not in broken.stdout:
            raise RuntimeError(f'{name}: missing named assertion {assertion}\n{broken.stdout}')
    print('Seven compiled production controls: exit 101; all seven named native assertions reached', flush=True)
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(log_dir / 'restored.log').write_text(restored.stdout)
if restored.returncode != 0 or '7 passed; 0 failed' not in restored.stdout:
    raise RuntimeError(f'restored target failed: {restored.returncode}\n{restored.stdout}')
assert source.read_bytes() == original
print('Restored original source bytes: exit 0; seven tests passed', flush=True)
