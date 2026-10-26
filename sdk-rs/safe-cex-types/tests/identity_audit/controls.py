from pathlib import Path
import subprocess,os
cargo=[os.environ.get('CARGO', 'cargo'),'test','--locked','--manifest-path','sdk-rs/Cargo.toml','--jobs','1']
def control(label,path,mutate,args,needle):
 p=Path(path); original=p.read_text(); wrong=mutate(original); assert wrong!=original,label
 try:
  p.write_text(wrong)
  result=subprocess.run(cargo+args,capture_output=True,text=True)
  output=result.stdout+result.stderr;Path('/tmp/p4-dynamic-'+label+'.log').write_text(output)
  assert result.returncode==101 and needle in output and 'could not compile' not in output,(label,output)
  print(label,'COMPILED, named failure:',needle,flush=True)
 finally:p.write_text(original)
 result=subprocess.run(cargo+args,capture_output=True,text=True)
 assert result.returncode==0,(label,result.stdout+result.stderr)
 print(label,'RESTORED PASS',flush=True)
def balance(s):
 for start,end in [('pub struct Balance {','\n}'),('struct WireBalance {','\n    }')]:
  i=s.index(start);j=s.index(end,i);block=s[i:j]
  for a,b in [('used','free'),('free','used')]:
   prefix='    pub ' if start.startswith('pub') else '        '
   block=block.replace(prefix+a+': f64,',('    ' if start.startswith('pub') else '        ')+'#[serde(rename = "'+b+'")]\n'+prefix+a+': f64,')
  s=s[:i]+block+s[j:]
 return s
control('decoded','sdk-rs/safe-cex-types/src/lib.rs',balance,['-p','safe-cex-types','--test','field_identity_supplement','decoded_balance'],'FIELD-IDENTITY Balance.used')
def rejection(s):
 i=s.index("impl<'de> serde::Deserialize<'de> for super::OrderStatus")
 j=s.index('\n    }',i)
 old=s[i:j]
 replacement=old[:old.index('            let value')]+ '            WireOrderStatus::deserialize(deserializer)\n        }'
 return s[:i]+replacement+s[j:]
control('rejected','sdk-rs/safe-cex-types/src/lib.rs',rejection,['-p','safe-cex-types','--test','field_identity_supplement','dynamic_rejected'],'DYNAMIC-REJECTION string union refuses non-string shapes')
control('operation','sdk-rs/safe-cex-types/src/symbol_converter.rs',lambda s:s.replace('return (&symbol[..symbol.len() - quote.len()], quote);','return (quote, &symbol[..symbol.len() - quote.len()]);'),['-p','safe-cex-types','--test','field_identity_supplement','dynamic_operation'],'DYNAMIC-OPERATION native base identity')
control('numeric','sdk-rs/safe-cex-types/tests/field_identity_supplement.rs',lambda s:s.replace('        number.to_bits()','        if number == 11.0 { 12.0_f64.to_bits() } else { number.to_bits() }'),['-p','safe-cex-types','--test','field_identity_supplement','supplement_nan'],'CAPTURE-BITS finite payload remains exact')
control('signed-zero','sdk-rs/safe-cex-types/tests/field_identity_supplement.rs',lambda s:s.replace('        number.to_bits()','        if number == 0.0 { 0.0_f64.to_bits() } else { number.to_bits() }'),['-p','safe-cex-types','--test','field_identity_supplement','supplement_nan'],'CAPTURE-BITS signed zero remains observable')
control('effect','sdk-rs/safe-cex-store/src/store.rs',lambda s:s.replace('                callback(&snapshot, from_worker)\n                    .map_err(|error| StoreNotificationError::from_subscriber(id, error))?;','                let _ = callback(&snapshot, from_worker)\n                    .map_err(|error| StoreNotificationError::from_subscriber(id, error));'),['-p','safe-cex-store','--test','notification','first_subscriber'],'first subscriber failure must skip second and reach caller recovery')
