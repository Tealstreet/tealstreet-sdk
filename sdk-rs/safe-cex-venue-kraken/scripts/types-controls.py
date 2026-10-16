#!/usr/bin/env python3
"""One production mutation per type row; named failure, byte restoration, passing rerun."""
from pathlib import Path
import argparse
import os
import subprocess
import tomllib

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
command = [cargo, 'test', '--locked', '--manifest-path', str(workspace / 'Cargo.toml'),
           '-p', 'safe-cex-venue-kraken', '--test', 'types', '--jobs', '1']

# Anchors include their owning declarations when a literal appears in several rows.
# Mutations affect production code; the oracle and assertions remain unchanged.
production = [
    ('futures.BASE_URL', 'src/types.rs', '"https://futures.kraken.com/derivatives/api"', '"https://wrong.example/derivatives/api"', 'table futures.BASE_URL'),
    ('futures.HISTORY_BASE_URL', 'src/types.rs', '"https://futures.kraken.com/api"', '"https://wrong.example/api"', 'table futures.HISTORY_BASE_URL'),
    ('futures.KrakenBaseUrl', 'src/types.rs', '    Derivatives,\n    History,',
     '    #[serde(rename(deserialize = "history"))]\n    Derivatives,\n    #[serde(rename(deserialize = "derivatives"))]\n    History,', 'KrakenBaseUrl decoded native variant'),
    ('futures.BASE_WS_URL', 'src/types.rs', '("frontendfutures", WS_NETWORKS)', '("wrong-role", WS_NETWORKS)', 'table futures.BASE_WS_URL'),
    ('futures.ENDPOINTS', 'src/types.rs', '("ACCOUNT_LOG", "/history/v3/account-log")', '("ACCOUNT_LOG", "/history/v2/account-log")', 'table futures.ENDPOINTS'),
    ('futures.PUBLIC_ENDPOINTS', 'src/types.rs', 'pub const PUBLIC_ENDPOINTS: &[&str] = &["/v3/instruments", "/v3/tickers", "/v3/orderbook"];',
     'pub const PUBLIC_ENDPOINTS: &[&str] = &["/v3/tickers", "/v3/instruments", "/v3/orderbook"];', 'array futures.PUBLIC_ENDPOINTS'),
    ('futures.ORDER_TYPE', 'src/types.rs', '("LIMIT", OrderType::Limit)', '("LIMIT", OrderType::Market)', 'table futures.ORDER_TYPE'),
    ('futures.ORDER_SIDE', 'src/types.rs', '("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)', '("BUY", OrderSide::Sell), ("SELL", OrderSide::Buy)', 'table futures.ORDER_SIDE'),
    ('futures.POSITION_SIDE', 'src/types.rs', '("LONG", PositionSide::Long)', '("LONG", PositionSide::Short)', 'table futures.POSITION_SIDE'),
    ('futures.TIME_IN_FORCE', 'src/types.rs', '("POST_ONLY", OrderTimeInForce::PostOnly)', '("POST_ONLY", OrderTimeInForce::GoodTillCancel)', 'table futures.TIME_IN_FORCE'),
    ('futures.ORDER_TRIGGER_TYPE', 'src/types.rs', '("MARK_PRICE", OrderStopTriggerType::MarkPrice)', '("MARK_PRICE", OrderStopTriggerType::IndexPrice)', 'table futures.ORDER_TRIGGER_TYPE'),
    ('spot.BASE_URL', 'src/spot_types.rs', '("testnet", "https://api.kraken.com")', '("testnet", "https://wrong.example")', 'table spot.BASE_URL'),
    ('spot.BASE_WS_URL', 'src/spot_types.rs', '("testnet", "wss://ws-auth.kraken.com/v2")', '("testnet", "wss://ws.kraken.com/v2")', 'lookup spot.BASE_WS_URL.private[testnet]'),
    ('spot.ENDPOINTS', 'src/spot_types.rs', '("AMEND_ORDER", "/0/private/AmendOrder")', '("AMEND_ORDER", "/0/private/AddOrder")', 'table spot.ENDPOINTS'),
    ('spot.PUBLIC_ENDPOINTS', 'src/spot_types.rs', 'pub const PUBLIC_ENDPOINTS: &[&str] = &[\n    "/0/public/AssetPairs",', 'pub const PUBLIC_ENDPOINTS: &[&str] = &[\n    "/0/private/Balance",', 'array spot.PUBLIC_ENDPOINTS'),
    ('spot.GRAFANA_ENDPOINTS_EVERY', 'src/spot_types.rs', 'pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[\n    "/0/private/AddOrder",', 'pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[\n    "/0/private/AmendOrder",', 'array spot.GRAFANA_ENDPOINTS_EVERY'),
    ('spot.GRAFANA_ENDPOINTS_PERIODIC', 'src/spot_types.rs', 'pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[\n    "/0/private/Balance",', 'pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[\n    "/0/private/TradeBalance",', 'array spot.GRAFANA_ENDPOINTS_PERIODIC'),
    ('spot.INTERVAL', 'src/spot_types.rs', '("12h", 240.0)', '("12h", 720.0)', 'table spot.INTERVAL'),
    ('spot.ORDER_TYPE', 'src/spot_types.rs', '("stop-loss-limit", OrderType::StopLoss)', '("stop-loss-limit", OrderType::Limit)', 'table spot.ORDER_TYPE'),
    ('spot.ORDER_SIDE', 'src/spot_types.rs', '("buy", OrderSide::Buy), ("sell", OrderSide::Sell)', '("buy", OrderSide::Sell), ("sell", OrderSide::Buy)', 'table spot.ORDER_SIDE'),
    ('spot.TIME_IN_FORCE', 'src/spot_types.rs', '("GTD", OrderTimeInForce::GoodTillCancel)', '("GTD", OrderTimeInForce::ImmediateOrCancel)', 'table spot.TIME_IN_FORCE'),
    ('spot.ASSET_ALIASES', 'src/spot_types.rs', '("XXBT", "BTC")', '("XXBT", "ETH")', 'table spot.ASSET_ALIASES'),
    ('spot.normalizeAsset', 'src/spot_types.rs', 'TableHit::Inherited(member) => return TableHit::Inherited(member),', 'TableHit::Inherited(_) => {}', 'normalizeAsset'),
]
supplemental = [
    ('typescript-enum', '../../packages/safe-cex/src/exchanges/kraken/kraken.types.ts',
     '  LIMIT: OrderType.Limit,', '  LIMIT: OrderType.Market,', 'table futures.ORDER_TYPE'),
    ('typescript-function', '../../packages/safe-cex/src/exchanges/kraken/krakenspot.types.ts',
     'return ASSET_ALIASES[asset] || asset;', 'return String(ASSET_ALIASES[asset] || asset);', 'normalizeAsset'),
    ('surrogate-loss', 'src/spot_types.rs', 'TableHit::Mapped(Cow::Borrowed(asset))',
     'TableHit::Mapped(Cow::Owned(String::from_utf16_lossy(asset).encode_utf16().collect()))', 'normalizeAsset'),
    ('signed-zero-loss', 'tests/types.rs', 'value.to_bits()', 'value.abs().to_bits()', 'negativeZero'),
    ('nan-sign-leak', 'tests/types.rs', '0x7ff8_0000_0000_0000', '0xfff8_0000_0000_0000', 'negativeNaN'),
]

parser = argparse.ArgumentParser()
parser.add_argument('--supplemental', action='store_true', help='also repeat the five previously proven source/capture controls')
parser.add_argument('--start-at', choices=[row[0] for row in production], help='resume at a production row after an interrupted run')
args = parser.parse_args()
# Real TOML census makes removal/duplication of a row control fail closed.
expected = set()
for family, source in [('futures', 'kraken'), ('spot', 'krakenspot')]:
    manifest = workspace / 'parity/manifest/exchanges/kraken' / (source + '.types.ts.toml')
    expected.update(family + '.' + row['ts'] for row in tomllib.loads(manifest.read_text())['symbol'])
assert len(production) == len(expected) and {row[0] for row in production} == expected, 'production mutation row census'
start = next((i for i, row in enumerate(production) if row[0] == args.start_at), 0)
controls = production[start:] + (supplemental if args.supplemental else [])

def run():
    # Each test process invokes Node ONCE for the full corpus through OnceLock.
    return subprocess.run(command, cwd=workspace, env=os.environ, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

baseline = run()
assert baseline.returncode == 0, baseline.stdout
for name, file, old, new, assertion in controls:
    target = crate / file
    original = target.read_bytes()
    source = original.decode()
    assert source.count(old) == 1, (name, 'mutation anchor count', source.count(old))
    try:
        target.write_text(source.replace(old, new))
        broken = run()
        assert broken.returncode not in (0, 126) and assertion in broken.stdout and 'test result: FAILED' in broken.stdout, (name, broken.stdout)
        print(f'{name}: failed named assertion {assertion}', flush=True)
    finally:
        target.write_bytes(original)
    restored = run()
    assert restored.returncode == 0, (name, 'restored failure', restored.stdout)
    print(f'{name}: restored pass', flush=True)
print(f'{len(controls)} mutations failed named assertions; {len(controls)} restorations passed')
