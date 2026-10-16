import os
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-okx/src/family_lifecycle.rs'
original = source.read_bytes()
log_dir = root / 'sdk-rs/target/okx-family-lifecycle-controls'
log_dir.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-okx', '--jobs', '2', '--test', 'family_lifecycle', '--', '--nocapture', '--test-threads=1']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
mutations = [
    ('constructor', '"okxeu"\n        } else {\n            "okx"', '"okx"\n        } else {\n            "okx"', 'CONSTRUCTOR native region selection'),
    ('demo', 'define(h, &forced, "testnet", (h.value.boolean)(true))?', 'define(h, &forced, "testnet", (h.value.boolean)(false))?', 'DEMO native forced testnet'),
    ('start', 'call(&h, "this.privateWebsocket.connectAndSubscribe", vec![])?;', 'call(&h, "this.businessWebsocket.connectAndSubscribe", vec![])?;', 'START native ordered load and connection effects'),
    ('tick', 'let callback = (h.callback)(Rc::new(move |_| call(&ops, "this.tick", vec![])));', 'let callback = (h.callback)(Rc::new(move |_| call(&ops, "this.log", vec![])));', 'TICK native retained callback resolves current member'),
    ('overlay', 'define(&h, &copy, "limits", (h.value.get)(&own, "limits")?)?', 'define(&h, &copy, "limits", (h.value.get)(&own, "id")?)?', 'OVERLAY native original limits identity'),
]
try:
    text = original.decode()
    patched = text
    for name, old, new, assertion in mutations:
        if patched.count(old) != 1:
            raise RuntimeError(f'{name}: expected exactly one production site')
        patched = patched.replace(old, new)
    source.write_text(patched)
    broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (log_dir / 'broken.log').write_text(broken.stdout)
    if broken.returncode != 101 or 'Finished `test`' not in broken.stdout or '0 passed; 5 failed' not in broken.stdout:
        raise RuntimeError(f'controls did not compile then fail: {broken.returncode}\n{broken.stdout}')
    for name, _, _, assertion in mutations:
        if assertion not in broken.stdout:
            raise RuntimeError(f'{name}: missing named assertion {assertion}\n{broken.stdout}')
    print('Five compiled production controls: exit 101; all five named assertions reached', flush=True)
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(log_dir / 'restored.log').write_text(restored.stdout)
if restored.returncode != 0 or '5 passed; 0 failed' not in restored.stdout:
    raise RuntimeError(f'restored target failed: {restored.returncode}\n{restored.stdout}')
assert source.read_bytes() == original
print('Restored original production bytes: exit 0; five tests passed', flush=True)
