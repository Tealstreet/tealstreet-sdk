"""Sequential, restored production mutations for the live constants witness."""
import os
from pathlib import Path
import subprocess
import tomllib
import argparse

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/constants.rs'
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-phemex', '--test', 'constants', '--jobs', '1']
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'

def run():
    return subprocess.run(command, capture_output=True, text=True)

controls = [
    ('BROKER_ID', '"Tealstreet"', '"OtherBroker"', 'ordered constant BROKER_ID'),
    ('BASE_URL', 'https://api.phemex.com', 'https://wrong.phemex.com', 'ordered constant BASE_URL'),
    ('BASE_WS_URL', '"public",', '"public_mutated",', 'ordered constant BASE_WS_URL'),
    ('ENDPOINTS', '/public/products', '/public/mutated', 'ordered constant ENDPOINTS'),
    ('GRAFANA_ENDPOINTS_PERIODIC', '[ENDPOINTS[12].1, ENDPOINTS[4].1]', '[ENDPOINTS[0].1, ENDPOINTS[4].1]', 'ordered constant GRAFANA_ENDPOINTS_PERIODIC'),
    ('GRAFANA_ENDPOINTS_EVERY', 'GRAFANA_ENDPOINTS_EVERY: [&str; 6] = [\n    ENDPOINTS[1].1,', 'GRAFANA_ENDPOINTS_EVERY: [&str; 6] = [\n    ENDPOINTS[0].1,', 'ordered constant GRAFANA_ENDPOINTS_EVERY'),
    ('PUBLIC_ENDPOINTS', 'PUBLIC_ENDPOINTS: [&str; 4] = [\n    ENDPOINTS[0].1,', 'PUBLIC_ENDPOINTS: [&str; 4] = [\n    ENDPOINTS[1].1,', 'ordered constant PUBLIC_ENDPOINTS'),
    ('ORDER_STATUS', '("new", OrderStatus::Open)', '("new", OrderStatus::Canceled)', 'ordered constant ORDER_STATUS'),
    ('ORDER_TYPE', '("market", "Market")', '("market", "Limit")', 'ordered constant ORDER_TYPE'),
    ('POSITION_SIDE', '("LONG", PositionSide::Long)', '("LONG", PositionSide::Short)', 'ordered constant POSITION_SIDE'),
    ('TIME_IN_FORCE', '("GoodTillCancel", OrderTimeInForce::GoodTillCancel)', '("GoodTillCancel", OrderTimeInForce::PostOnly)', 'ordered constant TIME_IN_FORCE'),
    ('ORDER_SIDE', '("BUY", OrderSide::Buy)', '("BUY", OrderSide::Sell)', 'ordered constant ORDER_SIDE'),
    ('RECV_WINDOW', '20_000.0', '20_001.0', 'RECV_WINDOW bits'),
    ('inherited lookup loss', 'pub use safe_cex_num::js_table_lookup as lookup;', '''pub fn lookup<'a, K: AsRef<str>, T>(table: &'a [(K, T)], key: &str) -> safe_cex_num::TableHit<&'a T> {
        match safe_cex_num::js_table_lookup(table, key) {
            safe_cex_num::TableHit::Inherited(_) => safe_cex_num::TableHit::Missing,
            other => other,
        }
    }''', 'table lookup BASE_URL.constructor'),
    ('prototype object lookup loss', 'pub use safe_cex_num::js_table_lookup as lookup;', '''pub fn lookup<'a, K: AsRef<str>, T>(table: &'a [(K, T)], key: &str) -> safe_cex_num::TableHit<&'a T> {
        match safe_cex_num::js_table_lookup(table, key) {
            safe_cex_num::TableHit::Inherited(safe_cex_num::ProtoMember::Prototype) => safe_cex_num::TableHit::Missing,
            other => other,
        }
    }''', 'table lookup BASE_URL.__proto__'),
]
manifests = [tomllib.loads(p.read_text()) for p in (crate.parent / 'parity/manifest/exchanges/phemex').glob('*.toml')]
ported_constants = {row['ts'] for m in manifests for row in m['symbol'] if row['kind'] == 'const' and row['status'] == 'ported'}
assert {row[0] for row in controls[:13]} == ported_constants, 'one production mutation per ported constant row'
parser = argparse.ArgumentParser()
parser.add_argument('--only', choices=[row[0] for row in controls])
args = parser.parse_args()
if args.only:
    controls = [row for row in controls if row[0] == args.only]

for name, before, after, assertion in controls:
    assert original.count(before) == 1, name
    try:
        source.write_text(original.replace(before, after))
        failed = run()
        output = failed.stdout + failed.stderr
        assert failed.returncode != 0 and assertion in output, (name, output)
        print(f'FAIL confirmed: {name}: {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = run()
    assert restored.returncode == 0, restored.stdout + restored.stderr
    print(f'PASS restored: {name}: constants_match_actual_typescript', flush=True)
