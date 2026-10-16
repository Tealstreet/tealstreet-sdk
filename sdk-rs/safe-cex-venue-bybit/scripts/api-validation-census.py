#!/usr/bin/env python3
"""Independent selected-symbol census; does not prove runtime parity."""
from pathlib import Path
import collections,json,subprocess,sys,tomllib
root=Path(__file__).resolve().parents[3]
base=sys.argv[1] if len(sys.argv)>1 else '41505c8707'
base=subprocess.check_output(['git','rev-parse',base],cwd=root,text=True).strip()
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
prefix='sdk-rs/parity/manifest/exchanges/bybit'
expected={
'BybitExchange.validateAccount':'divergent',
'BybitInverseExchange.validateAccount':'divergent',
'BybitSpotExchange.validateAccount':'divergent',
'HyrotraderExchange.validateAccount':'divergent',
'BybitExchange.fetchApiKeyInfo':'divergent',
'BybitInverseExchange.fetchApiKeyInfo':'divergent',
'BybitExchange.fetchAndCacheApiKeyInfo':'divergent',
'BybitInverseExchange#fetchAndCacheApiKeyInfo':'divergent',
'BybitExchange#updateBatchLimitBasedOnVipLevel':'ported',
'BybitInverseExchange#updateBatchLimitBasedOnVipLevel':'ported',
}
files=sorted(subprocess.check_output(['git','ls-tree','-r','--name-only',base,'--',prefix],cwd=root,text=True).splitlines())
assert len(files)==26,len(files)
before={};after={}
for f in files:
 a=tomllib.loads(subprocess.check_output(['git','show',base+':'+f],cwd=root,text=True))
 b=tomllib.loads((root/f).read_text())
 for rows,table in [(a['symbol'],before),(b['symbol'],after)]:
  for row in rows:
   key=(f,row['ts']);assert key not in table;table[key]=row
assert len(before)==520 and before.keys()==after.keys()
moves=[]
for key,row in after.items():
 old=before[key]
 if old['status']!=row['status']:
  assert old['status']=='pending' and expected.get(row['ts'])==row['status'],(key,old['status'],row['status'])
  assert row['rs'].startswith('safe-cex-venue-bybit/src/api_validation.rs::')
  if row['status']=='divergent': assert row['note'].startswith('CHOSEN:')
  moves.append(dict(file=key[0],symbol=row['ts'],before=old['status'],after=row['status'],target=row['rs']))
assert {r['symbol']:r['after'] for r in moves}==expected,moves
result=dict(measured_sha=head,base_sha=base,fresh_parent=subprocess.check_output(['git','rev-parse','41505c8707'],cwd=root,text=True).strip(),before=dict(collections.Counter(r['status'] for r in before.values())),after=dict(collections.Counter(r['status'] for r in after.values())),pending_to_ported=2,pending_to_divergent=8,moves=moves,manifest_floor=26,last_manifest=files[-1],row_floor=520)
p=root/'sdk-rs/safe-cex-venue-bybit/evidence/api-validation-census.json';p.parent.mkdir(exist_ok=True);p.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='moves'}))
