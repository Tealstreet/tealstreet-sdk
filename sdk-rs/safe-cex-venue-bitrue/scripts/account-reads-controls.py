"""One scoped target per control; each target invokes Node once for its corpus."""
from pathlib import Path
import os,re,subprocess,sys
root=Path(__file__).resolve().parents[3]
base=root/'sdk-rs/safe-cex-venue-bitrue'
paths={k:base/v for k,v in {'production':'src/account_reads.rs','oracle':'scripts/account-reads-oracle.cjs','test':'tests/account_reads.rs'}.items()}
original={k:p.read_text() for k,p in paths.items()}
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','account_reads','--locked','--jobs','1']
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
controls=[
 ('wrong endpoint','production','*key == "ACCOUNT"','*key == "ORDERS"','BITRUE-ACCOUNT-READ native request'),
 ('empty response','production','Ok(data) => Ok(data)','Ok(_data) => Ok(None)','BITRUE-ACCOUNT-READ native result'),
 ('drop emitter identity','production',')?;\n            Ok(None)',').map_err(|error| ExchangeError::new(error.message))?;\n            Ok(None)','BITRUE-ACCOUNT-READ emitter identity'),
 ('replace emitted value','production','response_message\n            } else','Some(&message)\n            } else','BITRUE-ACCOUNT-READ native emitted values'),
 ('clone fallback balance','production','BalanceRead::Stored(stored)','{ let _kept_variant = BalanceRead::Stored(std::cell::Ref::clone(&stored)); BalanceRead::Parsed(stored.clone()) }','BITRUE-BALANCE-READ native result'),
 ('same length replacement','oracle','process.stdout.write(JSON.stringify(','rows[0]=rows[1];process.stdout.write(JSON.stringify(','BITRUE-ACCOUNT-READ live input content'),
 ('same length reversal','oracle','process.stdout.write(JSON.stringify(','rows.reverse();process.stdout.write(JSON.stringify(','BITRUE-ACCOUNT-READ live input content'),
 ('balance replacement','oracle','process.stdout.write(JSON.stringify(','balances[0]=balances[1];process.stdout.write(JSON.stringify(','BITRUE-BALANCE-READ live input content'),
 ('balance reversal','oracle','process.stdout.write(JSON.stringify(','balances.reverse();process.stdout.write(JSON.stringify(','BITRUE-BALANCE-READ live input content'),
]
# Prove native result observations still discriminate when admission is removed.
controls.append(('empty response admission bypass','production','Ok(data) => Ok(data)','Ok(_data) => Ok(None)','BITRUE-ACCOUNT-READ native result'))
controls.append(('empty response result-guard bypass','production','Ok(data) => Ok(data)','Ok(_data) => Ok(None)','BITRUE-ACCOUNT-READ native branch counts'))
for label,key,old,new,named in controls:
 if len(sys.argv)>1 and label!=sys.argv[1]:continue
 assert original[key].count(old)==1,(label,old,original[key].count(old))
 try:
  paths[key].write_text(original[key].replace(old,new))
  if 'admission bypass' in label:
   test=original['test']
   test,n=re.subn(r'    assert_eq!\(rows.len\(\), 140, "BITRUE-ACCOUNT-READ source total"\);','',test);assert n==1
   test,n=re.subn(r'\s*assert_eq!\(\s*row.get\("input"\),\s*Some\(&json!\(\{"raw":raw,"fault":fault,"emitFails":emit_fails\}\)\),\s*"BITRUE-ACCOUNT-READ live input content"\s*\);','',test);assert n==1
   paths['test'].write_text(test)
  if 'result-guard bypass' in label:
   test,n=re.subn(r'\s*assert_eq!\(\s*Some\(&actual\),\s*row.get\("result"\),\s*"BITRUE-ACCOUNT-READ native result case \{\}",\s*i \* 10 \+ j \* 2 \+ k\s*\);','',original['test']);assert n==1
   test=test.replace('let actual = match result {','let _actual = match result {',1)
   paths['test'].write_text(test)
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode==101 and named in output and 'could not compile' not in output,output
  print('COMPILED EXIT101 '+label+' -> '+named,flush=True)
 finally:
  for key,path in paths.items():
   if path.read_text()!=original[key]:path.write_text(original[key])
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('RESTORED PASS '+label,flush=True)
