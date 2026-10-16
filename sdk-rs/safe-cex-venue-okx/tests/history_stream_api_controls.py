import os
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-okx/src/history_stream_api.rs'
original = source.read_bytes()
log_dir = root / 'sdk-rs/target/okx-history-stream-api-controls'
log_dir.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-okx', '--jobs', '2', '--test', 'history_stream_api', '--', '--nocapture', '--test-threads=1']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
mutations = [
 ('income', [('sorted(h.clone(), values, true)', 'sorted(h.clone(), values, false)')], 'INCOME native newest record first'),
 ('candles', [('sorted(h.clone(), candles, false)', 'sorted(h.clone(), candles, true)')], 'CANDLES native ascending timestamps'),
 ('candle-sub', [('("volume", "7")', '("volume", "5")')], 'SUBCANDLE native volume column seven'),
 ('book', [('(h.value.get)(&(h.value.get)(market, "precision")?, "amount")?', '(h.value.intrinsic)("multiply", vec![(h.value.get)(&(h.value.get)(market, "precision")?, "amount")?, (h.value.number)(10.)])?')], 'BOOK native contract multiplier'),
 ('factory', [('("OK-ACCESS-KEY", (h.value.get)(options, "key")?)', '("OK-ACCESS-KEY", (h.value.get)(options, "secret")?)')], 'FACTORY native signed API key'),
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
    if broken.returncode != 101 or 'Finished `test`' not in broken.stdout or '1 passed; 5 failed' not in broken.stdout:
        raise RuntimeError(f'controls did not compile then fail: {broken.returncode}\n{broken.stdout}')
    for name, _, assertion in mutations:
        if assertion not in broken.stdout:
            raise RuntimeError(f'{name}: missing native assertion {assertion}\n{broken.stdout}')
    print('Five compiled production controls: exit 101; all five named native assertions reached; response-phase control passes', flush=True)
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(log_dir / 'restored.log').write_text(restored.stdout)
if restored.returncode != 0 or '6 passed; 0 failed' not in restored.stdout:
    raise RuntimeError(f'restored target failed: {restored.returncode}\n{restored.stdout}')
assert source.read_bytes() == original
print('Restored exact source bytes: exit 0; six tests passed', flush=True)
