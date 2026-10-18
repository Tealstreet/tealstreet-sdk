#!/usr/bin/env python3
"""Each source effect must compile, fail its named native contract, then restore."""
from pathlib import Path
import subprocess,json
ROOT=Path(__file__).resolve().parents[3]
PKG=ROOT/'sdk-rs/safe-cex-transport'
RUNNER=PKG/'scripts/accepted-retry-native.mjs'
INV=Path('/tmp/transport-retry-control-inventory.json');RESULT=Path('/tmp/transport-retry-control-results.json')
D='signature_rejection::resend_on_rejected_data';S='signature_rejection::resend_on_signature_rejection';R='signature_rejection::signature_rejected_response';E='signature_rejection::retry_network_reads_and_signature_rejection';W='signing::RemoteSignerRequest'
CASES=[
 ('signature_rejection.rs','data-approval',{D,S},'resend_on_rejected_data','    Ok(true)','    Ok(false)','function output mismatch'),
 ('signature_rejection.rs','error-approval',{R,E},'signature_rejected_response','    Ok(true)','    Ok(false)','function output mismatch'),
 ('signature_rejection.rs','effective-error-config',{S},'resend_on_signature_rejection','&execution.config','&crate::RestRequest::default()','function output mismatch'),
 ('signature_rejection.rs','write-replay-config',{E},'retry_network_reads_and_signature_rejection','let mut next = (*error.request).clone();','let mut next = (*error.request).clone(); next.url = "/disconnected".into();','function output mismatch'),
 ('signature_rejection.rs','write-timeout-budget',{E},'retry_network_reads_and_signature_rejection','next.timeout_ms = Some(remaining.min(i128::from(i64::MAX)) as i64);','next.timeout_ms = Some(timeout);','function output mismatch'),
 ('signature_rejection.rs','read-network-retry',{E},'retry_network_reads_and_signature_rejection','if error.is_retryable_network() {','if error.is_retryable_network() && false {','function output mismatch'),
 ('signing.rs','response-origin',{W},None,'Self::WsAuth { .. } => serde_json::from_str(json).map(RemoteSignerResponse::WsAuth),','Self::WsAuth { .. } => serde_json::from_str(json).map(RemoteSignerResponse::Http),','native observation mismatch'),
 ('signing.rs','ws-account-field',{W},None,'account_name: header.account_name,','account_name: header.account_name.filter(|_| false),','native observation mismatch'),
 ('signing.rs','unknown-operation-admission',{W},None,'"http" => serde_json::from_str(raw.get())','"http" | "future-kind" => serde_json::from_str(raw.get())','accepted {'),
]

def native(label,expected,restored,message):
 RESULT.unlink(missing_ok=True)
 done=subprocess.run(['node',str(RUNNER),'--inventory='+str(INV),'--out='+str(RESULT)],cwd=ROOT,capture_output=True,text=True)
 suffix='restored' if restored else 'disconnected'
 log=Path('/tmp/transport-retry-'+label+'-'+suffix+'.log');log.write_text(done.stdout+done.stderr)
 assert RESULT.exists(),f'{label}: fresh receipts required; compiler failure is not behavioral proof: {log}'
 rows=json.loads(RESULT.read_text());assert len(rows)==5,(label,len(rows))
 failed=[r for r in rows if r['failures']]
 if restored:assert done.returncode==0 and not failed,log
 else:
  assert done.returncode!=0 and failed,log
  assert {r['name']for r in failed}=={'safe_cex_transport::'+n for n in expected},(label,failed)
  assert all(message in e for r in failed for e in r['failures']),(label,failed)
 RESULT.with_name('transport-retry-'+label+'-'+suffix+'-results.json').write_bytes(RESULT.read_bytes())
 print(label+': '+('restored five receipts PASS'if restored else 'compiled named behavioral FAIL'),flush=True)

prepare=subprocess.run(['node',str(ROOT/'sdk-rs/safe-cex-parity/scripts/accepted-set.mjs'),'--package=safe-cex-transport','--inventory','--jobs=1'],cwd=ROOT/'sdk-rs',capture_output=True,text=True)
Path('/tmp/transport-retry-control-discovery.log').write_text(prepare.stdout+prepare.stderr)
assert prepare.returncode==0,prepare.stdout+prepare.stderr
metadata=subprocess.run(['cargo','metadata','--locked','--format-version=1'],cwd=ROOT/'sdk-rs',capture_output=True,text=True);assert metadata.returncode==0,metadata.stderr
INV.write_bytes((Path(json.loads(metadata.stdout)['target_directory'])/'accepted-set-inventory.json').read_bytes())
originals={PKG/'src'/f:(PKG/'src'/f).read_bytes()for f,*_ in CASES}
try:
 for filename,label,expected,function,old,new,message in CASES:
  path=PKG/'src'/filename;source=originals[path].decode()
  if function:
   start=source.index('pub '+('async 'if function.startswith('retry_network')else '')+'fn '+function+'(')
   end=source.find('\npub ',start+1);end=len(source)if end<0 else end
   segment=source[start:end];assert segment.count(old)==1,(label,segment.count(old))
   changed=source[:start]+segment.replace(old,new)+source[end:]
  else:
   assert source.count(old)==1,(label,source.count(old));changed=source.replace(old,new)
  assert changed.count('\n')==source.count('\n'),label
  try:path.write_text(changed);native(label,expected,False,message)
  finally:path.write_bytes(originals[path])
  native(label,expected,True,message)
finally:
 for path,source in originals.items():path.write_bytes(source)
assert all(p.read_bytes()==source for p,source in originals.items())
print('Nine independent compiling effect/admission controls; every exact restoration PASS',flush=True)
