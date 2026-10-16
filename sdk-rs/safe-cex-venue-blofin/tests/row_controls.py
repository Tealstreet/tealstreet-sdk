"""One sequential production fail/restore/pass mutation per mapped types row.

Each cargo run invokes Node once for the entire corpus. No per-case processes.
"""
import os
import pathlib
import re
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-blofin'
SOURCE = CRATE / 'src/types.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path',
           str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin',
           '--locked', '--jobs', '1', '--test', 'types']
CONSTANTS = {
    'RECV_WINDOW': ('20000.0', '20001.0'),
    'BROKER_ID': ('b3cdedc0a20880ba', 'b3cdedc0a20880bb'),
    'USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY': ('false', 'true'),
    'BASE_URL': ('https://openapi.blofin.com', 'https://mutation.invalid'),
    'BASE_WS_URL': ('wss://openapi.blofin.com/ws/public', 'wss://mutation.invalid'),
    'ENDPOINTS': ('/api/v1/market/instruments', '/mutation'),
    'GRAFANA_ENDPOINTS_EVERY': ('ENDPOINTS["CANCEL_ORDERS"]', 'ENDPOINTS["CANCEL_ALGO_ORDER"]'),
    'GRAFANA_ENDPOINTS_PERIODIC': ('ENDPOINTS["BALANCE"]', 'ENDPOINTS["POSITIONS"]'),
    'INTERVAL': ('"1m": "1m"', '"1m": "M1"'),
    'ORDER_STATUS': ('"live": "open"', '"live": "closed"'),
    'ORDER_TYPE': ('"market": "market"', '"market": "limit"'),
    'REVERSE_ORDER_TYPE': ('"market": "market"', '"market": "limit"'),
    'ORDER_SIDE': ('"buy": "buy"', '"buy": "sell"'),
    'POSITION_SIDE': ('"long": "long"', '"long": "short"'),
}
TYPES = {
    'FETCH_POSITION_RESPONSE': 'FetchPositionResponse',
    'POSITION_MODE_RESPONSE': 'PositionModeResponse',
    'FETCH_MARGIN_MODE_RESPONSE': 'FetchMarginModeResponse',
    'FETCH_BATCH_LEV_RESPONSE': 'FetchBatchLevResponse',
    'ORDERS_RESPONSE': 'OrdersResponse',
    'POSITIONS_RESPONSE': 'PositionsResponse',
}


def mutation(source, name):
    if name in CONSTANTS:
        start = re.search(r'pub (?:const|static) ' + name + r':', source).start()
        following = re.search(r'\npub (?:const|static) |\nfn present', source[start:])
        end = start + following.start()
        old, new = CONSTANTS[name]
        segment = source[start:end]
        assert segment.count(old) == 1, f'{name}: mutation anchor count'
        return source[:start] + segment.replace(old, new, 1) + source[end:], f'constant_{name}'
    native = TYPES[name]
    start = source.index(f'pub struct {native} {{')
    end = source.index(f'impl JsNumberOutput for {native} {{', start)
    segment = source[start:end]
    # Swap both encoding and decoding: the native-field assertion must catch it.
    assert segment.count('"code"') == segment.count('"msg"') == 2
    segment = segment.replace('"code"', '"__swap__"').replace('"msg"', '"code"').replace('"__swap__"', '"msg"')
    return source[:start] + segment + source[end:], f'decoded_{native}_code'


def main():
    assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'
    manifest = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.types.ts.toml').read_text())
    names = [row['ts'] for row in manifest['symbol'] if row['status'] != 'pending']
    assert set(names) == set(CONSTANTS) | set(TYPES), 'mapped_row_mutation_census'
    original = SOURCE.read_text()
    logs = ROOT / 'sdk-rs/target/blofin-row-controls'
    logs.mkdir(parents=True, exist_ok=True)
    for name in names:
        changed, assertion = mutation(original, name)
        try:
            SOURCE.write_text(changed)
            result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            (logs / f'{name}.fail.log').write_text(result.stdout)
            # 126 is UNKNOWN, and compiler failures cannot discharge runtime proof.
            assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
            print(f'{name}: FAIL {assertion}; exit=101', flush=True)
        finally:
            SOURCE.write_text(original)
        result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / f'{name}.restored.log').write_text(result.stdout)
        assert result.returncode == 0, result.stdout
        print(f'{name}: RESTORED exit=0', flush=True)
    print(f'Production row controls: {len(names)} failed by name and restored', flush=True)


if __name__ == '__main__':
    main()
