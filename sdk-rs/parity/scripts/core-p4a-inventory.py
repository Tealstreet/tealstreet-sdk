"""Count the eleven assigned CORE/P4a manifests with Python 3.11 tomllib."""
import collections
from pathlib import Path
import subprocess
import sys
import tomllib

root = Path(__file__).resolve().parents[3]
revision = sys.argv[1] if len(sys.argv) > 1 else 'HEAD'
sha = subprocess.check_output(['git', 'rev-parse', revision], cwd=root, text=True).strip()
files = ['exchanges/registry.ts.toml', 'exchanges/registry.native.ts.toml', 'utils/nativePositionSize.ts.toml']
files += [f'script-api-core/order/{name}.ts.toml' for name in [
    'ScriptValidationError', 'breakevenPrice', 'downSizeReduceOrder', 'notionalSizing',
    'orderPricing', 'prepareOrder', 'rounding', 'shouldRoundSizeDown',
]]
print(f'# CORE/P4a inventory at {sha}\n')
print('Enumeration iteration 1: the fixed eleven assigned manifests. No other manifests are included.\n')
print('| Manifest | Total | Pending | Ported | Divergent | Not applicable |')
print('| --- | ---: | ---: | ---: | ---: | ---: |')
totals = collections.Counter()
rows = []
for file in files:
    source = subprocess.check_output(['git', 'show', f'{sha}:sdk-rs/parity/manifest/{file}'], cwd=root, text=True)
    symbols = tomllib.loads(source)['symbol']
    counts = collections.Counter(symbol['status'] for symbol in symbols)
    totals.update(counts)
    print(f'| {file} | {len(symbols)} | {counts["pending"]} | {counts["ported"]} | {counts["divergent"]} | {counts["not-applicable"]} |')
    rows += [(file, symbol) for symbol in symbols]
print(f'| TOTAL | {sum(totals.values())} | {totals["pending"]} | {totals["ported"]} | {totals["divergent"]} | {totals["not-applicable"]} |\n')
print('Census records manifest status; it is not proof of runtime parity.\n')
print('| Manifest | Symbol | Status |')
print('| --- | --- | --- |')
for file, symbol in rows:
    print(f'| {file} | {symbol["ts"]} | {symbol["status"]} |')
