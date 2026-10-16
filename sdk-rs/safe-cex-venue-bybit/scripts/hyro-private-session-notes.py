"""Canonical divergence notes within the hard ten-file variant partition."""
from pathlib import Path
import re,tomllib
ROOT=Path(__file__).resolve().parents[3];SDK=ROOT/'sdk-rs'
files=['bybitinverse.exchange.ts','bybitinverse.ws-private.ts','bybitinverse.ws-public.ts','bybitspot.exchange.ts','bybitspot.ws-private.ts','bybitspot.ws-public.ts','hyrotrader.exchange.ts','hyrotrader.order-recovery.ts','hyrotrader.ws-private.ts','bybitdemo.exchange.ts']
count=0
for name in files:
 manifest=tomllib.loads((SDK/'parity/manifest/exchanges/bybit'/(name+'.toml')).read_text())
 for row in manifest['symbol']:
  if row['status']=='divergent':
   assert re.match(r'^(FORCED|CHOSEN|MIXED|CORRECTED|FINDING)\b',row['note']),(name,row['ts'])
   targets=row['rs'] if isinstance(row['rs'],list)else[row['rs']];assert targets and all(targets)
   for target in targets:assert (SDK/target.split('::')[0]).is_file(),target
   count+=1
print('Variant divergence notes:',count,'canonical buckets; ten assigned manifests; plain Bybit excluded.')
