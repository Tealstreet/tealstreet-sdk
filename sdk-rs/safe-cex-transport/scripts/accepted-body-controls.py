#!/usr/bin/env python3
"""Review controls: each changed operation must compile and cause native output failure."""
from pathlib import Path
import subprocess,json,os
ROOT=Path(__file__).resolve().parents[3]
PKG=ROOT/'sdk-rs/safe-cex-transport'
RUNNER=PKG/'scripts/accepted-body-native.mjs'
INV=Path('/tmp/transport-body-control-inventory.json');RESULT=Path('/tmp/transport-body-control-results.json')
B='provider::RestBody';R='provider::RestRequest';S='provider::RestResponseBody';A='secondary::apply_secondary_options'
CASES=[
 ('provider.rs','body-borrow',B,{B},'Self::Untransformed(value) => Ok(value),','Self::Untransformed(value) => { let _ = value; Err(TransportError::Host("disconnected borrow".into())) },'),
 ('provider.rs','body-signing-phase',B,{B},'Self::Transformed(_) => Ok(Self::Transformed(TransformedData { value })),','Self::Transformed(_) => Ok(Self::Untransformed(value)),'),
 ('provider.rs','body-transform',B,{B,R},'Self::Encoded(_) | Self::Transformed(_) => Ok(self.clone()),','Self::Encoded(_) | Self::Transformed(_) => Ok(Self::Untransformed(serde_json::Value::Null)),'),
 ('provider.rs','body-wire',B,{B,R},'Self::Encoded(bytes) => Ok(bytes.clone()),','Self::Encoded(bytes) => Ok(bytes.iter().rev().copied().collect()),'),
 ('provider.rs','request-headers',R,{R},'self.headers = headers;','self.headers = headers.into_iter().rev().collect();'),
 ('provider.rs','request-wire-null',R,{R},'Some(RestBody::Transformed(data)) if data.value().is_null() => None,','Some(RestBody::Transformed(data)) if data.value().is_null() => Some(RestBody::Encoded(b"null".to_vec())),'),
 ('provider.rs','response-current-data',S,{S},'serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())','{ let _ = bytes; serde_json::Value::String(String::new()) }'),
 ('provider.rs','response-transform-phase',S,{S},'Self::Transformed(transform(self.into_data()))','Self::Untransformed(transform(self.into_data()))'),
 ('provider.rs','response-fallible-phase',S,{S},'transform(self.into_data()).map(Self::Transformed)','transform(self.into_data()).map(Self::Untransformed)'),
 ('provider.rs','response-physical-bom',S,{S},"text.strip_prefix('\\u{feff}').unwrap_or(&text).to_owned()",'text.to_string()'),
 ('provider.rs','response-model-read',S,{S},'.and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value)','.and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value).map(|_| serde_json::Value::Null)'),
 ('secondary.rs','secondary-native-copy',A,{A},'let mut secondary = options.clone();','let mut secondary = options.clone(); if let Some(nested) = &mut secondary.polymarket { nested.signer_type = Some(0.0); }'),
]

def run_native(label,required,expected,restored):
 RESULT.unlink(missing_ok=True)
 done=subprocess.run(['node',str(RUNNER),'--inventory='+str(INV),'--out='+str(RESULT)],cwd=ROOT,capture_output=True,text=True)
 suffix='restored' if restored else 'disconnected'
 log=Path('/tmp/transport-body-'+label+'-'+suffix+'.log');log.write_text(done.stdout+done.stderr)
 assert RESULT.exists(), f'{label}: fresh receipts required; compilation failure is not a behavioral control: {log}'
 rows=json.loads(RESULT.read_text());assert len(rows)==5,(label,len(rows))
 failed=[r for r in rows if r['failures']]
 if restored:
  assert done.returncode==0 and not failed,log
 else:
  assert done.returncode!=0 and failed,log
  assert {r['name'] for r in failed}=={'safe_cex_transport::'+n for n in expected},(label,failed)
  assert 'safe_cex_transport::'+required in [r['name'] for r in failed]
  assert all('function output mismatch' in e for r in failed for e in r['failures']),(label,failed)
 RESULT.with_name('transport-body-'+label+'-'+suffix+'-results.json').write_bytes(RESULT.read_bytes())
 print(label+': '+('restored five receipts PASS' if restored else 'compiled named output FAIL'),flush=True)

def selector(restored):
 name='maintained_typescript_secondary_options_empty_strings_and_proxy_precedence'
 env=os.environ.copy();env['SAFE_CEX_TYPESCRIPT_PATH']=str(ROOT/'node_modules/typescript')
 done=subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-transport','--test','secondary_options',name,'--','--exact','--include-ignored'],cwd=ROOT/'sdk-rs',env=env,capture_output=True,text=True)
 log=Path('/tmp/transport-body-secondary-proxy-'+('restored' if restored else 'disconnected')+'.log');log.write_text(done.stdout+done.stderr)
 assert 'could not compile' not in done.stderr,log
 if restored:assert done.returncode==0 and '1 passed; 0 failed' in done.stdout,log
 else:assert done.returncode==101 and f'test {name} ... FAILED' in done.stdout,log
 print('secondary-proxy: '+('restored actual source/native 729 PASS' if restored else 'compiled named source/native FAIL'),flush=True)

prepare=subprocess.run(['node',str(ROOT/'sdk-rs/safe-cex-parity/scripts/accepted-set.mjs'),'--package=safe-cex-transport','--inventory','--jobs=1'],cwd=ROOT/'sdk-rs',capture_output=True,text=True)
Path('/tmp/transport-body-control-discovery.log').write_text(prepare.stdout+prepare.stderr)
assert prepare.returncode==0,prepare.stdout+prepare.stderr
metadata=subprocess.run(['cargo','metadata','--locked','--format-version=1'],cwd=ROOT/'sdk-rs',capture_output=True,text=True)
assert metadata.returncode==0,metadata.stderr
INV.write_bytes((Path(json.loads(metadata.stdout)['target_directory'])/'accepted-set-inventory.json').read_bytes())
originals={PKG/'src'/f:(PKG/'src'/f).read_bytes() for f,*_ in CASES}
try:
 for filename,label,required,expected,old,new in CASES:
  path=PKG/'src'/filename;source=originals[path].decode();assert source.count(old)==1,(label,source.count(old))
  try:
   path.write_text(source.replace(old,new));run_native(label,required,expected,False)
  finally:path.write_bytes(originals[path])
  run_native(label,required,expected,True)
 path=PKG/'src/secondary.rs';source=originals[path].decode();old='.or_else(|| provider.public_cors_proxy())';assert source.count(old)==1
 try:
  path.write_text(source.replace(old,'.or_else(|| { let _ = provider; Some("disconnected://proxy".into()) })'));selector(False)
 finally:path.write_bytes(originals[path])
 selector(True)
finally:
 for path,source in originals.items():path.write_bytes(source)
assert all(p.read_bytes()==source for p,source in originals.items())
print('12 independent compiled API faults plus actual-source selector control; exact restoration PASS',flush=True)
