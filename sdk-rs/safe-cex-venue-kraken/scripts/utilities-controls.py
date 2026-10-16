#!/usr/bin/env python3
"""One production mutation per utility row; serial, corpus-batched live witnesses."""
from pathlib import Path
import argparse
import os
import subprocess
import tomllib

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--locked', '--manifest-path', str(workspace / 'Cargo.toml'),
           '-p', 'safe-cex-venue-kraken', '--test', 'utilities', '--jobs', '1']
production = [
    ('fills.toSeconds', 'src/fills.rs', '(ms / 1000.0).floor()', '(ms / 1000.0).trunc()', 'fills.toSeconds raw='),
    ('fills.num', 'src/fills.rs', 'if parsed.is_finite() { parsed } else { 0.0 }', 'if parsed.is_finite() { parsed } else { 1.0 }', 'fills.num raw='),
    ('fills.optionalNum', 'src/fills.rs', 'Ok(parsed.is_finite().then_some(parsed))', 'Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))', 'fills.optionalNum raw='),
    ('fills.mapKrakenFill', 'src/fills.rs', 'price: num(row.get("price"))?,\n        qty: num(row.get("size"))?,', 'price: num(row.get("size"))?,\n        qty: num(row.get("price"))?,', 'fills.mapKrakenFill decoded native fields'),
    ('fills.mapKrakenFills', 'src/fills.rs', 'for row in rows {', 'for row in rows.into_iter().rev() {', 'fills.mapKrakenFills decoded native fields'),
    ('income.isPositionSizeLeg', 'src/income.rs', 'Ok(!asset.is_empty() && asset == contract)', 'Ok(!asset.is_empty() && asset != contract)', 'income.isPositionSizeLeg raw='),
    ('income.num', 'src/income.rs', 'Ok(parsed.is_finite().then_some(parsed))', 'Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))', 'income.num raw='),
    ('income.toMs', 'src/income.rs', 'if parsed.is_finite() { parsed } else { 0.0 }', 'if parsed.is_finite() { parsed / 1000.0 } else { 0.0 }', 'income.toMs raw='),
    ('income.mapKrakenAccountLogRow', 'src/income.rs', 'let realized_pnl = num(row.get("realized_pnl"))?;\n    let realized_funding = num(row.get("realized_funding"))?;', 'let realized_pnl = num(row.get("realized_funding"))?;\n    let realized_funding = num(row.get("realized_pnl"))?;', 'income.mapKrakenAccountLogRow decoded native fields'),
    ('income.mapKrakenAccountLog', 'src/income.rs', 'for row in rows {', 'for row in rows.into_iter().rev() {', 'income.mapKrakenAccountLog decoded native fields'),
]
supplemental = [
    ('typescript-num', '../../packages/safe-cex/src/exchanges/kraken/kraken.fills.ts',
     'return Number.isFinite(parsed) ? parsed : 0;', 'return Number.isFinite(parsed) ? parsed : 1;', 'fills.num raw='),
    ('signed-zero', 'tests/utilities.rs', 'value.to_bits()', 'value.abs().to_bits()', 'signed zero capture'),
    ('nan-sign', 'tests/utilities.rs', '0x7ff8000000000000', '0xfff8000000000000', 'NaN capture'),
]
parser = argparse.ArgumentParser()
parser.add_argument('--supplemental', action='store_true')
parser.add_argument('--start-at', choices=[row[0] for row in production])
args = parser.parse_args()
expected=set()
for part in ['fills','income']:
    manifest=workspace / 'parity/manifest/exchanges/kraken' / f'kraken.{part}.ts.toml'
    expected.update(part+'.'+row['ts'] for row in tomllib.loads(manifest.read_text())['symbol'])
assert len(production)==len(expected) and {row[0] for row in production}==expected, 'utility production mutation row census'
start=next((i for i,row in enumerate(production) if row[0]==args.start_at),0)
controls=production[start:]+(supplemental if args.supplemental else [])
def run():
    return subprocess.run(command,cwd=workspace,env=os.environ,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run()
assert baseline.returncode==0, baseline.stdout
for name,file,old,new,assertion in controls:
    target=crate/file
    original=target.read_bytes()
    source=original.decode()
    assert source.count(old)==1,(name,'mutation anchor count',source.count(old))
    try:
        target.write_text(source.replace(old,new))
        broken=run()
        assert broken.returncode not in (0,126) and assertion in broken.stdout and 'test result: FAILED' in broken.stdout,(name,broken.stdout)
        print(f'{name}: failed named assertion {assertion}',flush=True)
    finally:
        target.write_bytes(original)
    restored=run()
    assert restored.returncode==0,(name,'restored failure',restored.stdout)
    print(f'{name}: restored pass',flush=True)
print(f'{len(controls)} mutations failed named assertions; {len(controls)} restorations passed')
