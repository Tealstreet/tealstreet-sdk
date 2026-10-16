#!/usr/bin/env python3
"""Sequential production and corpus controls; source restored after each run."""
import os,re,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
source=crate/'src/account_reads.rs'; target=crate/'tests/account_reads.rs'; oracle=crate/'tests/account_reads_oracle.cjs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-bingx','--test','account_reads','--jobs','1','--locked']
controls=[
 ('drop-subscriber-identity',[(source,'.await?;\n    let data','.await.map_err(|mut error| { error.store_subscriber_ids.clear(); error })?;\n    let data')],'propagated identity'),
 ('request-override',[(source,'..Default::default()','skip_remote_signing: true, ..Default::default()')],'native config'),
 ('prefer-other-asset',[(source,'Some("USDT")','Some("USDC")')],'native value'),
 ('wrong-user-field',[(source,'balance.get("userId")','balance.get("asset")')],'native value'),
 ('drop-error-identity',[(source,'.await?;\n    let data','.await.map_err(|error| ExchangeError::new(error.message))?;\n    let data')],'error name'),
]
# Both responses have already been fully consumed by the shared ingress decoder.
for label,op in [('consuming-empty','rows.clear();'),('consuming-subset','rows.truncate(1);'),('extra-winning-row','rows.insert(0, json!({"asset":"USDT","userId":"extra"}));')]:
 controls.append((label,[(source,'let data = input','let mut data = input'),(source,'let code = data.get("code");',f'if let Some(Value::Array(rows)) = data.get_mut("data") {{ {op} }}\n    let code = data.get("code");')], 'error outcome' if label=='consuming-empty' else ('success outcome' if label=='extra-winning-row' else 'native value')))
for label,expr in [('empty-object','json!({})'),('subset-object','json!({"userId":balance.expect("truthy balance exists").get("userId")})'),('extra-object','{ let mut value = balance.expect("truthy balance exists").clone(); if let Value::Object(ref mut object) = value { object.insert("extra".into(), json!(true)); } value }')]:
 controls.append((label,[(source,'balance.expect("truthy balance exists").clone()',expr)],'native value'))
for operation in ['splice(0)','splice(1)','push(cases[0])']:
 for bypass in [False,True]:
  edits=[(oracle,' process.stdout.write',f' cases.{operation};\n process.stdout.write')]
  if bypass:
   guard=re.search(r'assert_eq!\(\s*cases.len\(\),\s*68,\s*"BINGX-READ source cases"\s*\);',target.read_text()).group()
   edits.append((target,guard,''))
   content=re.search(r'    assert_eq!\(\s*cases\s*\.iter\(\).*?BINGX-READ cases input content.*?;\n',target.read_text(),re.S).group()
   edits.append((target,content,''))
  controls.append((f'{operation}-bypass-{bypass}',edits,'executed method counts' if bypass else 'source cases'))
for name,condition in [('erase-negative-zero','number == 0.0'),('erase-infinity','!number.is_finite()')]:
 expr='{ let mut value = balance.expect("truthy balance exists").clone(); if let Some(number) = value.get("userId").and_then(js_value_as_f64) { if '+condition+' { value["userId"] = json!(0); } } value }'
 controls.append((name,[(source,'balance.expect("truthy balance exists").clone()',expr)],'numeric native bits'))
for operation in ['splice(0)','splice(1)','push(numeric[0])']:
 for bypass in [False,True]:
  edits=[(oracle,' process.stdout.write',f' numeric.{operation};\n process.stdout.write')]
  if bypass:
   guard=re.search(r'assert_eq!\(\s*numeric.len\(\),\s*10,\s*"BINGX-READ numeric source total"\s*\);',target.read_text()).group()
   edits.append((target,guard,''))
   content=re.search(r'    assert_eq!\(\s*numeric\s*\.iter\(\).*?BINGX-READ numeric input content.*?;\n',target.read_text(),re.S).group()
   edits.append((target,content,''))
  controls.append((f'numeric-{operation}-bypass-{bypass}',edits,'numeric comparisons' if bypass else 'numeric source total'))
for group in ['cases','numeric','boundary']:
 for change in [f'{group}[0] = {group}[1]',f'{group}.reverse()']:
  controls.append((f'{group}-same-length-{change}',[(oracle,' process.stdout.write',f' {change};\n process.stdout.write')],f'{group} input content'))
for name,edits,assertion in controls:
 originals={p:p.read_bytes() for p,_,_ in edits}
 try:
  for p,before,after in edits:
   text=p.read_text();assert text.count(before)==1,(name,before)
   p.write_text(text.replace(before,after))
  result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101 and f'BINGX-READ {assertion}' in result.stdout and 'test result: FAILED' in result.stdout,result.stdout
  print(f'{name}: exit101 BINGX-READ {assertion}',flush=True)
 finally:
  for p,text in originals.items():p.write_bytes(text)
 result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 assert result.returncode==0 and '1 passed' in result.stdout,result.stdout
 print('restored 1 passed',flush=True)
