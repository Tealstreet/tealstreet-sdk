#!/usr/bin/env python3
"""Compile one production fault per socket helper, require RED, restore exact bytes."""
import pathlib, subprocess, json
root=pathlib.Path(__file__).resolve().parents[3]
base=root/'sdk-rs/safe-cex-venue-binance/src/inverse'
runner=root/'sdk-rs/safe-cex-venue-binance/scripts/inverse-socket-helper-contracts.mjs'
controls=[]
for module in ['ws_private','ws_public']:
 controls.extend([
  (module,'flag','Ok(rt.this.get('+('key' if module=='ws_private' else 'name')+')?.truthy())','Ok(!rt.this.get('+('key' if module=='ws_private' else 'name')+')?.truthy())'),
  (module,'call','owner.get('+('name' if module=='ws_private' else 'method')+')?.call(owner.clone(), args)','owner.get('+('name' if module=='ws_private' else 'method')+')?.call(owner.clone(), args).map(|_| Js::Undefined)'),
  (module,'global','rt.global(name)?.call(Js::Undefined, args)','rt.global(name)?.call(Js::Undefined, args).map(|_| Js::Undefined)'),
 ])
controls.extend([
 ('ws_private','optional','if value.is_nullish() {\n        Ok(Js::Undefined)','if value.is_nullish() {\n        Ok(Js::Null)'),
 ('ws_private','send','let text = global(rt, "JSON.stringify", vec![value])?;','let text = global(rt, "JSON.stringify", vec![{ let _ = value; Js::Null }])?;'),
 ('ws_private','timeout','        delay,\n    ))','        delay + 1.,\n    ))'),
 ('ws_public','subscription','Js::Number(id + 1.)','Js::Number(id + 2.)'),
 ('ws_public','tickers','patch.set(target, Js::Number(value.parse_float()?))?;','patch.set(target, Js::Number(value.parse_float()? + 1.))?;'),
])
assert len(controls)==11
receipts=[]
for module,name,before,after in controls:
 path=base/(module+'.rs');original=path.read_bytes();text=original.decode();assert text.count(before)==1,(module,name)
 owner='safe_cex_venue_binance::inverse::'+module+'::'+name
 log=pathlib.Path('/tmp/port-txflow-socket-helper-control-'+module+'-'+name+'.log')
 if log.exists():
  previous=log.read_text()
  if '1 failed owners' in previous and previous.rstrip().endswith('Inverse socket helpers: 1 real native owners pass. Scope excludes other owners.'):
   receipts.append({'owner':owner,'compiledRed':True,'exactRestoredGreen':True,'log':str(log)})
   print(owner+' retained checked RED -> exact restored GREEN receipt',flush=True)
   continue
 try:
  path.write_text(text.replace(before,after))
  with log.open('w') as output:
   result=subprocess.run(['node',str(runner),'--only='+owner],cwd=root,stdout=output,stderr=subprocess.STDOUT)
  body=log.read_text()
  assert result.returncode!=0 and 'production runtime results' in body and '1 failed owners' in body,(owner,body[-3000:])
  assert 'could not compile' not in body,(owner,body[-3000:])
 finally:path.write_bytes(original)
 with log.open('a') as output:
  result=subprocess.run(['node',str(runner),'--only='+owner],cwd=root,stdout=output,stderr=subprocess.STDOUT)
 assert result.returncode==0,(owner,log.read_text()[-3000:])
 receipts.append({'owner':owner,'compiledRed':True,'exactRestoredGreen':True,'log':str(log)})
 print(owner+' compiled RED -> exact restored GREEN',flush=True)
pathlib.Path('/tmp/port-txflow-socket-helper-controls.json').write_text(json.dumps(receipts,indent=2)+'\n')
