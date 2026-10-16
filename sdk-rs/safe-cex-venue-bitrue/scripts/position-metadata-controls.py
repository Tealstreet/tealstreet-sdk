"""Compiling production controls; one Node invocation per whole 100-case corpus."""
from pathlib import Path
import os, subprocess
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitrue/src/position_metadata.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','position_metadata','--locked','--jobs','1']
strict='position.position_type.is_number()\n                    && js_value_as_f64(position.position_type) == Some(2.0)'
controls=[
 ('mutable field as lookup key','.find(|(key, _)| key == symbol)','.find(|(_, entry)| &entry.borrow().symbol == symbol)','BITRUE-METADATA stable key output count'),
 ('mutable field as presence key','.any(|(key, _)| key == symbol)','.any(|(_, entry)| &entry.borrow().symbol == symbol)','BITRUE-METADATA stable key fetch count'),
 ('detached returned handles','                .map(|(_, entry)| entry.clone())','                .map(|(_, entry)| std::rc::Rc::new(std::cell::RefCell::new(entry.borrow().clone())))','BITRUE-METADATA shared record identity'),
 ('wrong leverage','                leverage,','                leverage: leverage + 1.0,','BITRUE-METADATA native fields'),
 ('coerced margin type',strict,'js_number(Some(position.position_type)).map_err(coercion_error)? == 2.0 || (false && js_value_as_f64(position.position_type) == Some(2.0))','BITRUE-METADATA native fields'),
 ('disposal before cache replacement','    let missing = supplied.is_none()','    if is_disposed() { return Ok(Vec::new()); }\n    let missing = supplied.is_none()','BITRUE-METADATA'),
 ('empty output','        .collect())','        .take(0).collect())','BITRUE-METADATA native collection length'),
 ('subset output','        .collect())','        .take(1).collect())','BITRUE-METADATA native collection length'),
 ('extra output','        .collect())','        .chain(cache.borrow().first().map(|(_, entry)| entry.clone())).collect())','BITRUE-METADATA native collection length'),
]
for label,old,new,assertion in controls:
 assert original.count(old)==1,(label,old)
 try:
  source.write_text(original.replace(old,new))
  result=subprocess.run(command,capture_output=True,text=True)
  output=result.stdout+result.stderr
  assert result.returncode==101 and assertion in output and 'could not compile' not in output,output
  print('COMPILED EXIT101 '+label+' -> '+assertion,flush=True)
 finally:source.write_text(original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('RESTORED PASS '+label,flush=True)
