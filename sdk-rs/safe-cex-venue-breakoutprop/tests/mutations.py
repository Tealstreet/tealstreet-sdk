"""Scoped, sequential negative controls. Always restore source bytes."""
import os
import sys
import re
import tomllib
from pathlib import Path
import subprocess
from decoder_mutations import transform_schema

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
cargo = '/opt/homebrew/opt/rustup/bin/cargo'
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = [cargo, 'test', '--manifest-path', str(workspace / 'Cargo.toml'), '-p', 'safe-cex-venue-breakoutprop', '--test', 'parity', '--jobs', '1']


def probe(name, file, transform, test, diagnostic):
    original = file.read_bytes()
    mutated = transform(original.decode()).encode()
    assert mutated != original, name
    try:
        file.write_bytes(mutated)
        result = subprocess.run(command + [test, '--', '--exact'], env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and diagnostic in output and 'test result: FAILED' in output, (name, output)
        print(f'{name}: FAIL exit={result.returncode} ({diagnostic})', flush=True)
    finally:
        file.write_bytes(original)
    result = subprocess.run(command + [test, '--', '--exact'], env=env, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    print(f'{name}: restored PASS', flush=True)


def hardcode_bool(source):
    changed = transform_schema(source, 'PortfolioOrder', lambda body: body.replace('    pub final_status: bool,', '    #[serde(deserialize_with = "always_true")]\n    pub final_status: bool,', 1))
    return changed + "\nfn always_true<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> { let _ = bool::deserialize(d)?; Ok(true) }\n"


def swap_bid_ask(source):
    return source.replace('#[serde(rename = "ask")]', '#[serde(rename = "TEMP")]').replace('#[serde(rename = "bid")]', '#[serde(rename = "ask")]').replace('#[serde(rename = "TEMP")]', '#[serde(rename = "bid")]')



def mutate_constant(name, old, new):
    def transform(source):
        pattern = r'(pub const ' + name + r'\b[^;]*;)'
        matches = list(re.finditer(pattern, source))
        assert len(matches) == 1, name
        span = matches[0]
        body = span[0]
        assert old in body, (name, old)
        return source[:span.start()] + body.replace(old, new, 1) + source[span.end():]
    return transform


def swap_strings(record, first, second):
    def transform(source):
        def swap(body):
            a, b = '#[serde(rename = "' + first + '")]', '#[serde(rename = "' + second + '")]'
            assert a in body and b in body, record
            return body.replace(a, 'SWAP_MARKER').replace(b, a).replace('SWAP_MARKER', b)
        return transform_schema(source, record, swap)
    return transform


constants = [
    ('RECV_WINDOW', '20_000.0', '20_001.0'),
    ('BASE_URL', 'https://breakout.', 'https://mutated.'),
    ('ENDPOINTS', '"/login"', '"/mutated-login"'),
    ('GRAFANA_ENDPOINTS_EVERY', 'accounts/{accountCode}/orders"', 'accounts/{accountCode}/mutated"'),
    ('GRAFANA_ENDPOINTS_PERIODIC', '/portfolio', '/mutated'),
    ('INTERVAL', '("1m", "m")', '("1m", "mutated")'),
    ('ORDER_TYPE', '("LIMIT", "limit")', '("LIMIT", "market")'),
    ('ORDER_SIDE', '("BUY", "buy")', '("BUY", "sell")'),
    ('ORDER_STATUS', '("WORKING", "open")', '("WORKING", "mutated")'),
    ('POSITION_SIDE', '("BUY", "long")', '("BUY", "short")'),
]
records = [
    ('LOGIN_RESPONSE', 'LoginResponse', 'sessionToken', 'timeout'),
    ('USER_RESPONSE', 'UserDetail', 'login', 'domain'),
    ('PORTFOLIO_RESPONSE', 'PortfolioBalance', 'account', 'currency'),
    ('INSTRUMENTS_RESPONSE', 'Instrument', 'description', 'currency'),
    ('INSTRUMENT_DETAILS_RESPONSE', 'InstrumentDetails', 'account', 'symbol'),
    ('TICKER_RESPONSE', 'QuoteEvent', 'symbol', 'type'),
    ('CANDLE_RESPONSE', 'CandleEvent', 'symbol', 'candleType'),
]
manifest = workspace / 'parity/manifest/exchanges/breakoutprop/breakoutprop.types.ts.toml'
rows = tomllib.loads(manifest.read_text())['symbol']
assert {r['ts'] for r in rows} == {r[0] for r in constants + records}, 'one production mutation per types row'
if '--supplemental' not in sys.argv:
    for name, old, new in constants:
        probe(name, crate/'src/types.rs', mutate_constant(name, old, new), 'constants_and_lookup', 'constant_' + name)
    for name, record, first, second in records:
        probe(name, crate/'src/types.rs', swap_strings(record, first, second), 'native_fields_and_bits', 'row_' + name)
    probe('parseDxSessionTimeoutMs', crate/'src/exchange.rs', lambda s:s.replace('minutes >= 60.0', 'minutes > 60.0'), 'source_helpers', 'session_timeout_bits:')
    probe('toDxOrderType', crate/'src/exchange.rs', lambda s:s.replace('Ok("LIMIT")', 'Ok("MARKET")'), 'source_helpers', 'order_type_branch:')
else:
    probe('same-type bid/ask serde swap', crate/'src/types.rs', swap_bid_ask, 'native_fields_and_bits', 'native_fields_and_bits:')
    probe('hardcoded portfolio finalStatus', crate/'src/types.rs', hardcode_bool, 'native_fields_and_bits', 'native_fields_and_bits:')
    probe('timeout minute boundary', crate/'src/exchange.rs', lambda s:s.replace('minutes >= 60.0', 'minutes > 60.0'), 'source_helpers', 'session_timeout_bits:')
    probe('constant drift', crate/'src/types.rs', lambda s:s.replace('20_000.0', '20_001.0'), 'constants_and_lookup', 'constant_RECV_WINDOW')
    probe('inherited-field omission', crate/'tests/oracle.cjs', lambda s:s.replace("  assert(!node.heritageClauses?.length, 'inherited-field census refusal');", ''), 'native_fields_and_bits', 'inherited-field control')
    probe('lossy opaque JSON visitor', crate/'src/types.rs', lambda s:s.replace('safe_cex_num::numeric_ingress::js_deserialize_passthrough(decoder)?', 'serde_json::Value::deserialize(decoder)?'), 'native_fields_and_bits', 'native_fields_and_bits')
