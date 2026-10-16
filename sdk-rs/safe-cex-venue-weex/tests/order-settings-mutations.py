import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
crate = root / 'sdk-rs/safe-cex-venue-weex'
out = root / 'sdk-rs/target/weex-order-settings'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'order_settings']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('parse-order-id', 'order_result.rs', 'host, result, "orderId"', 'host, result, "clientOrderId"', 'native_parse_output::"order_id"'),
    ('cache-symbol', 'symbol_settings.rs', ') -> Result<V, E> {\n    (runtime.cached)(host, symbol)\n}', ') -> Result<V, E> {\n    (runtime.cached)(host, &symbol[..0])\n}', 'native_settings_symbols::"cache_hit"'),
    ('default-leverage', 'symbol_settings.rs', 'cross_leverage: 20.0,', 'cross_leverage: 21.0,', 'native_settings_output::"default_after_await"'),
]
results = []
for name, filename, before, after, assertion in controls:
    source = crate / 'src' / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, name
    try:
        source.write_text(text.replace(before, after))
        broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (name + '-broken.log')).write_text(broken.stdout)
        assert broken.returncode == 101, (name, broken.returncode)
        assert assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout, (name, broken.stdout)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / (name + '-restored.log')).write_text(restored.stdout)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    row = {'control': name, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode}
    results.append(row)
    print(json.dumps(row), flush=True)
(out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
