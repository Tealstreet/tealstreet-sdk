"""Sequential, restoring mutations; run only at a slice boundary, never concurrently."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
command = [cargo, 'test', '--manifest-path', str(workspace / 'Cargo.toml'),
           '-p', 'safe-cex-venue-orderly', '--test', 'source_parity', '--jobs', '1', '--locked']
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'

def run():
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

baseline = run()
assert baseline.returncode == 0, baseline.stdout
controls = [
    ('src/types.rs', '20_000.0', '20_001.0', 'constant/RECV_WINDOW'),
    ('src/types.rs', 'ab82cb09-cfec-4473-80a3-b740779d0644', 'wrong-broker', 'constant/BROKER_ID'),
    ('src/types.rs', 'e708a644-9ce5-46c6-b50c-74b15f62d8ca', 'wrong-testnet', 'constant/TESTNET_BROKER_ID'),
    ('src/types.rs', 'https://api-evm.orderly.org', 'https://wrong.example', 'constant/BASE_URL'),
    ('src/types.rs', 'wss://ws-private-evm.orderly.org/v2/ws/private/stream', 'wss://wrong.example', 'constant/BASE_WS_URL'),
    ('src/types.rs', '("ACCOUNT", "/v1/client/info")', '("ACCOUNT", "/v1/client/holding")', 'constant/ENDPOINTS'),
    ('src/types.rs', 'ENDPOINTS[8].1,', 'ENDPOINTS[0].1,', 'constant/GRAFANA_ENDPOINTS_EVERY'),
    ('src/types.rs', 'ENDPOINTS[18].1,', 'ENDPOINTS[17].1,', 'constant/GRAFANA_ENDPOINTS_PERIODIC'),
    ('src/types.rs', '("LIMIT", OrderType::Limit)', '("LIMIT", OrderType::Market)', 'constant/ORDER_TYPE'),
    ('src/types.rs', '("BUY", OrderSide::Buy)', '("BUY", OrderSide::Sell)', 'constant/ORDER_SIDE'),
    ('src/utils.rs', 'parts.next().unwrap_or("undefined")', 'parts.next().unwrap_or("")', 'normalize/'),
    ('src/utils.rs', 'replacen("USDC", "", 1)', 'replace("USDC", "")', 'reverse/USDCBTCUSDC'),
    ('tests/source-oracle.cjs', 's.charCodeAt(i)', '(s.charCodeAt(i) === 0xd800 ? 0xfffd : s.charCodeAt(i))', 'source surrogate code units'),
    ('tests/source-oracle.cjs', "utils.normalizeSymbol.cache.set('cache-control', 'injected-normalize');", '', 'source public cache mutation'),
]
for file, original, replacement, assertion in controls:
    path = crate / file
    saved = path.read_bytes()
    source = saved.decode()
    assert original in source, (file, original)
    try:
        path.write_text(source.replace(original, replacement, 1))
        result = run()
        assert result.returncode != 0 and assertion in result.stdout and 'panicked at' in result.stdout, result.stdout
        print(f'FAIL observed: {assertion}', flush=True)
    finally:
        path.write_bytes(saved)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    print(f'PASS restored: {assertion}', flush=True)
print(f'{len(controls)} mutations rejected; {len(controls)} restorations passed')
