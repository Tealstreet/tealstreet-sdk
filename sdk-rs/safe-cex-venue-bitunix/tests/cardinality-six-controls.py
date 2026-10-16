"""Six-shape follow-up: serial scoped tests, one Node process per corpus."""
from pathlib import Path
import subprocess,re
root=Path(__file__).resolve().parents[3]
crate=root/'sdk-rs/safe-cex-venue-bitunix'
logs=root/'sdk-rs/target/bitunix-cardinality-six';logs.mkdir(exist_ok=True)
controls=[]
for target,label,assertion in [('utils','utility corpus census','SHA native comparison count'),('order_ids','order ID corpus census','ID native leg type comparison count'),('value_helpers','value helper corpus census','VALUE parse native comparison count')]:
 p=crate/'tests'/(target+'.rs');original=p.read_text()
 bypass=original.replace('let rows: Vec<Value>', 'let mut rows: Vec<Value>')
 bypass,n=re.subn(r'    assert_eq!\(rows.len\(\), \d+, "'+label+r'"\);', '', bypass)
 assert n==1,label
 if target=='value_helpers':
  filtering='rows.retain(|row| row["raw"].as_str().is_some_and(|raw| raw.contains("\\\\ud800")));'
 else:
  filtering='rows.retain(|row| String::from_utf16(&serde_json::from_value::<Vec<u16>>(row["units"].clone()).unwrap()).is_err());'
 for mode,operation in [('empty','rows.clear();'),('subset','rows.remove(0);'),('extra','rows.push(rows[0].clone());'),('refusals-only',filtering)]:
  changed=bypass.replace('    let mut refused = 0;', '    '+operation+'\n    let mut refused = 0;')
  controls.append((target+'-'+mode,{p:changed},target,assertion))
# Reuse the independently compiled response-wrapper mutant definition without
# running the earlier controls or creating additional Node subprocesses.
prior=crate/'tests/cardinality-controls.py'
namespace={'__file__':str(prior)}
exec(prior.read_text().split('controls=[')[0],namespace)
wire=namespace['wire'];oracle=namespace['oracle'];test=namespace['test'];supplement=namespace['supplement'];constants=namespace['constants']
original=namespace['original'];replacement=namespace['replacement'];alias=namespace['alias']
without_both=re.sub(r'    // SOURCE_CARDINALITY_START.*?    // SOURCE_CARDINALITY_END','',original[test],flags=re.S)
without_both=re.sub(r'    // SOURCE_OUTPUT_CARDINALITY_START.*?    // SOURCE_OUTPUT_CARDINALITY_END','',without_both,flags=re.S)
for name,operation in [('empty','rows.clear()'),('subset','rows.truncate(1)'),('extra','if let Some(first)=rows.first().cloned() { rows.push(first); }')]:
 controls.append(('wire-native-'+name+'-both-source-guards-bypassed',{wire:original[wire].replace(alias,replacement(operation)),test:without_both},'wire','wire native response count'))
controls.extend([
 ('wire-expected-subset-both-guards-bypassed',{supplement:namespace['source_missing'],test:without_both},'wire','wire native response count'),
 ('wire-interval-subcorpus-empty',{oracle:original[oracle].replace('intervals: interval.types.map(t => t.value)','intervals: []')},'wire','wire source interval count'),
 ('wire-supplement-empty',{supplement:original[supplement].replace('JSON.stringify({...original,supplement})','JSON.stringify({...original,supplement:[]})')},'wire','wire supplement count'),
 ('wire-expected-fields-empty',{oracle:original[oracle].replace('Object.fromEntries(Object.entries(row).map(([key, value]) => [key, observe(value)]))','({})')},'wire','position decoded fields'),
 ('constant-extra',{constants:original[constants].replace('&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)]','&[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell), ("EXTRA", OrderSide::Buy)]')},'constants','constant witness ORDER_SIDE'),
])
originals={p:p.read_text() for _,changes,_,_ in controls for p in changes}
for name,changes,target,assertion in controls:
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--jobs','1','--test',target,'--','--nocapture']
 try:
  for p,changed in changes.items():
   assert changed!=originals[p],name
   p.write_text(changed)
  result=subprocess.run(command,text=True,capture_output=True)
  (logs/(name+'-fail.log')).write_text(result.stdout+result.stderr)
  assert result.returncode==101 and 'Finished `test`' in result.stderr and assertion in result.stdout+result.stderr,(name,result.returncode,result.stdout+result.stderr)
  print('FAIL compiled 101: '+name+' / '+assertion,flush=True)
 finally:
  for p in changes:p.write_text(originals[p])
 result=subprocess.run(command,text=True,capture_output=True)
 (logs/(name+'-pass.log')).write_text(result.stdout+result.stderr)
 assert result.returncode==0,(name,result.stdout+result.stderr)
 print('PASS restored: '+name,flush=True)
print('controls proved:',len(controls),flush=True)
