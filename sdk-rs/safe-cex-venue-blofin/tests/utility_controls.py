"""Three production row mutations, sequential whole-corpus fail/restore tests."""
import os
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/utility.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'utility', '--', '--nocapture']
MUTATIONS = {
    'getNormalizedInstType': ('.to_uppercase()', '.to_lowercase()', 'utility_getNormalizedInstType_native'),
    'isBlofinLinearSwapMarket': ('== "linear"', '== "inverse"', 'utility_isBlofinLinearSwapMarket_native'),
    'isKnownNonSwapInstrument': ('inst_type != "SWAP"', 'inst_type == "SWAP"', 'utility_isKnownNonSwapInstrument_native'),
    'BlofinExchange#parsePositivePrice': ('parsed > 0.0', 'parsed >= 0.0', 'utility_parsePositivePrice_native'),
    'BlofinExchange#mapBlofinTriggerType': ('Some("mark") => Some(OrderStopTriggerType::MarkPrice)', 'Some("mark") => Some(OrderStopTriggerType::LastPrice)', 'utility_mapBlofinTriggerType_native'),
    'BlofinExchange#toBlofinTriggerType': ('Some("index_price") => "index"', 'Some("index_price") => "mark"', 'utility_toBlofinTriggerType_native'),
    'BlofinExchange#isAlreadyGoneCancelRow': ('Some("1000")', 'Some("1001")', 'utility_isAlreadyGoneCancelRow_native'),
    'BlofinExchange#isAlgoOrder': ('"trailing_stop_market"', '"disabled"', 'utility_isAlgoOrder_native'),
}
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
rows = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.exchange.ts.toml').read_text())['symbol']
assert {r['ts'] for r in rows if r['status'] != 'pending' and any('/utility.rs::' in target for target in (r['rs'] if isinstance(r['rs'], list) else [r['rs']]))} == set(MUTATIONS), 'utility_row_mutation_census'
original = SOURCE.read_text()
logs = ROOT / 'sdk-rs/target/blofin-utility-controls'
logs.mkdir(exist_ok=True)
for name, (old, new, assertion) in MUTATIONS.items():
    assert original.count(old) == 1, name + '_anchor'
    try:
        SOURCE.write_text(original.replace(old, new))
        result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / (name + '.fail.log')).write_text(result.stdout)
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print(f'{name}: FAIL {assertion} exit=101', flush=True)
    finally:
        SOURCE.write_text(original)
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.restored.log')).write_text(result.stdout)
    assert result.returncode == 0, result.stdout
    print(f'{name}: RESTORED exit=0', flush=True)
