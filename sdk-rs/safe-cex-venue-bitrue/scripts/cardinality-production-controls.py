"""Production collection controls; original baseline or repaired source-guard bypass."""
from pathlib import Path
import subprocess,os,sys,tarfile,io
root=Path(__file__).resolve().parents[3];venue=root/'sdk-rs/safe-cex-venue-bitrue'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
original_mode='--original-extra' in sys.argv
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--locked','--jobs','1']
old_tests={}
if original_mode:
 data=subprocess.check_output(['git','archive','10c73e82aeeb8d61db4a11de8ebbb6f90002040d','sdk-rs/safe-cex-venue-bitrue/tests'])
 with tarfile.open(fileobj=io.BytesIO(data)) as archive:
  for item in archive:
   if item.isfile():old_tests[root/item.name]=archive.extractfile(item).read().decode()
cases=[]
for function,needle,variable in [('accounts','Ok(Some(futures_accounts))','futures_accounts'),('snapshots','Ok(Some(snapshots))','snapshots')]:
 for shape,n in [('empty',0),('subset',1),('extra',None)]:
  extra=f'{{ let mut result = {variable}.clone(); result.extend({variable}.first().cloned()); Ok(Some(result)) }}'
  replacement=extra if n is None else f'Ok(Some({variable}.into_iter().take({n}).collect()))'
  cases.append(('accounts',function,shape,venue/'src/account_helpers.rs',needle,replacement,'account native parity:'))
for shape,n in [('empty',0),('subset',1),('extra',None)]:
 replacement='let mut result = currencies.clone(); if let Some(value) = currencies.values().next() { result.insert("EXTRA".into(), value.clone()); } balance.currencies = Some(result);' if n is None else f'balance.currencies = Some(currencies.into_iter().take({n}).collect());'
 cases.append(('balance','currencies',shape,venue/'src/balance.rs','balance.currencies = Some(currencies);',replacement,'balance native parity:'))
needle='FetchAccountResponseDataWire::deserialize(safe_cex_types::strict_decode::object(\n                    decoder,\n                ))'
for shape,n in [('empty',0),('subset',1),('extra',None)]:
 change='if let Some(value) = decoded.account.first().cloned() { decoded.account.push(value); }' if n is None else f'decoded.account.truncate({n});'
 cases.append(('wire','decoded accounts',shape,venue/'src/wire.rs',needle,needle+'.map(|mut decoded| { '+change+' decoded })','native type parity:'))
p=venue/'src/types.rs';s=p.read_text();start=s.index('pub const BASE_WS_URL:');end=s.index('\npub const ENDPOINTS:',start);needle=s[start:end]
for shape,n,body in [('empty',0,''),('subset',1,'("public", "wss://fmarket-ws.bitrue.com/kline-api/ws"),'),('extra',3,'("public", "wss://fmarket-ws.bitrue.com/kline-api/ws"),("private", "wss://fapiws.bitrue.com/stream"),("extra", "wss://extra"),')]:
 cases.append(('configuration','WS URLs',shape,p,needle,f'pub const BASE_WS_URL: [(&str, &str); {n}] = [{body}];','configuration parity: BASE_WS_URL'))
needle='pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];'
for shape,n,body in [('empty',0,''),('subset',1,'("BUY", OrderSide::Buy)'),('extra',3,'("BUY", OrderSide::Buy),("SELL", OrderSide::Sell),("EXTRA", OrderSide::Buy)')]:
 cases.append(('tables','side lookup entries',shape,p,needle,f'pub const ORDER_SIDE: [(&str, OrderSide); {n}] = [{body}];','table parity: ORDER_SIDE'))
p=root/'sdk-rs/safe-cex-store/src/store.rs'
needle="pub fn markets(&self) -> Ref<'_, [Market]> {\n        Ref::map(self.state.borrow(), |state| state.markets())\n    }"
for shape,expression in [('empty','&state.markets()[..0]'),('subset','&state.markets()[..state.markets().len().min(1)]'),('extra',None)]:
 replacement=needle.replace('state.markets())',expression+')') if expression else needle.replace('        Ref::map(', '        { let mut state = self.state.borrow_mut(); if let Some(value) = state.data.markets.first().cloned() { state.data.markets.push(value); } }\n        Ref::map(')
 cases.append(('leverage','Store markets',shape,p,needle,replacement,'leverage native parity:'))
for target,label,shape,p,old,new,original_assertion in cases:
 if original_mode and shape!='extra':continue
 test=venue/'tests'/(target+'.rs');helper=venue/'tests/cardinality.rs.inc'
 originals={q:q.read_text() for q in [p,test,helper]}
 try:
  assert originals[p].count(old)==1,(label,old);p.write_text(originals[p].replace(old,new))
  if original_mode:test.write_text(old_tests[test])
  else:
   # Bypass the entire frozen source census, retaining counter reset and usage.
   h=originals[helper];start=h.index('    assert_eq!(array_cardinalities(value), expected,');end=h.index('\n}',start)
   helper.write_text(h[:start]+'    let _ = (array_cardinalities(value), expected);'+h[end:])
   if target=='leverage':
    t=originals[test];start=t.index('        assert_eq!(\n            expected_markets.len(),\n            2,');end=t.index('        );',start)+len('        );')
    test.write_text(t[:start]+t[end:])
   if target=='tables':
    t=originals[test];start=t.index('        assert_eq!(\n            expected_keys.len(),\n            expected_count,');end=t.index('        );',start)+len('        );')
    test.write_text(t[:start]+'        let _ = expected_count;'+t[end:])
  r=subprocess.run(command+['--test',target],capture_output=True,text=True);output=r.stdout+r.stderr
  if original_mode and target in ['tables','leverage']:
   assert r.returncode==0,output
   print('ORIGINAL FALSE GREEN: '+label+' '+shape,flush=True)
  else:
   assertion=original_assertion if original_mode else 'BITRUE-LEVERAGE-CARDINALITY retained markets' if target=='leverage' else 'BITRUE-NATIVE-CARDINALITY'
   assert r.returncode==101 and assertion in output and 'could not compile' not in output,output
   print('FAIL101 PROVED: '+label+' '+shape+' -> '+assertion,flush=True)
 finally:
  for q,text in originals.items():q.write_text(text)
 r=subprocess.run(command+['--test',target],capture_output=True,text=True)
 assert r.returncode==0,r.stdout+r.stderr
 print('RESTORED PASS: '+label+' '+shape,flush=True)
