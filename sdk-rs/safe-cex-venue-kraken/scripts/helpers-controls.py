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
           '-p', 'safe-cex-venue-kraken', '--test', 'helpers', '--jobs', '1']
production = [
    ('kraken.exchange.parseTriggerSignal','src/helpers.rs','"spot" => Some(OrderStopTriggerType::IndexPrice)','"spot" => Some(OrderStopTriggerType::MarkPrice)','parseTriggerSignal native enum'),
    ('kraken.api.sort','src/helpers.rs','collator.compare(left, right)','collator.compare(right, left)','sort localeCompare'),
    ('kraken.api.createKrakenApiError','src/helpers.rs','error.metadata.response = response.cloned();\n    error.metadata.original_error = cause.cloned();','error.metadata.response = cause.cloned();\n    error.metadata.original_error = response.cloned();','createKrakenApiError response native field'),
    ('krakenspot.exchange.createKrakenSpotClientOrderId','src/helpers.rs','usize::from(*byte & 15)','usize::from(*byte & 7)','createKrakenSpotClientOrderId actual nanoid output'),
]
supplemental = [
    ('pool-growth','src/helpers.rs','pool_next = (target * 16).min(32768);','pool_next = (target * 8).min(32768);','createKrakenSpotClientOrderId entropy requests'),
    ('typescript-trigger','../../packages/safe-cex/src/exchanges/kraken/kraken.exchange.ts',"case 'spot':\n      return OrderStopTriggerType.IndexPrice;","case 'spot':\n      return OrderStopTriggerType.LastPrice;",'parseTriggerSignal native enum'),
    ('typescript-sort-magnitude','../../packages/safe-cex/src/exchanges/kraken/kraken.api.ts','return a.localeCompare(b);','return 2 * a.localeCompare(b);','sort localeCompare'),
]
parser = argparse.ArgumentParser()
parser.add_argument('--supplemental', action='store_true')
parser.add_argument('--start-at', choices=[row[0] for row in production])
args = parser.parse_args()
expected=set()
for part in ['kraken.api','kraken.exchange','krakenspot.exchange']:
    manifest=workspace / 'parity/manifest/exchanges/kraken' / f'{part}.ts.toml'
    expected.update(part+'.'+row['ts'] for row in tomllib.loads(manifest.read_text())['symbol'] if row['rs'].startswith('safe-cex-venue-kraken/src/helpers.rs::'))
assert len(production)==len(expected) and {row[0] for row in production}==expected, 'helper production mutation row census'
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
