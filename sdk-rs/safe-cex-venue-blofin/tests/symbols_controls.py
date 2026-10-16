"""Three production row mutations, sequential whole-corpus fail/restore tests."""
import os
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/symbols.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'symbols', '--', '--nocapture']
MUTATIONS = {
    'BlofinExchange.symbolToMarketId': ('market.id.clone()', 'market.symbol.clone()', 'symbols_symbolToMarketId_native'),
    'BlofinExchange.marketIdToSymbol': ('market.symbol.clone()', 'market.id.clone()', 'symbols_marketIdToSymbol_native'),
}
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
rows = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.exchange.ts.toml').read_text())['symbol']
assert {r['ts'] for r in rows if r['status'] != 'pending' and '/symbols.rs::' in r['rs']} == set(MUTATIONS), 'symbols_row_mutation_census'
original = SOURCE.read_text()
logs = ROOT / 'sdk-rs/target/blofin-symbols-controls'
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
