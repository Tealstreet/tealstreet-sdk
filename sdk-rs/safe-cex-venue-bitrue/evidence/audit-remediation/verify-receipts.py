import pathlib,subprocess,tomllib,collections,json,hashlib,re
# Requires Python 3.11+ (tomllib). Run from the checkout root.
source_sha='9f1be06793a067d64a079a9dbfebae84774cd01a';base='694e07f7e049a5a7047c41e994a0fc854dc78a59';head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();out=pathlib.Path('sdk-rs/safe-cex-venue-bitrue/evidence/audit-remediation');gated=out/source_sha[:10]
records=json.loads((gated/'gates.json').read_text());expected=['fmt','check','tests','host-clippy','wasm-clippy','sync-mutated','sync-restored','leverage-mutated','leverage-restored','update-mutated','update-restored','raw-mutated','raw-restored','balance-mutated','balance-restored','position-mutated','position-restored','cache-mutated','cache-restored','sync-emit-mutated','sync-emit-restored','native-key-mutated','native-key-restored'];assert [r['name'] for r in records]==expected
for r in records:
 assert r['sha']==source_sha
 assert hashlib.sha256((gated/r['log']).read_bytes()).hexdigest()==r['log_sha256']
 assert r['exit']==(101 if r['name'].endswith('-mutated') else 0)
controls=json.loads((gated/'controls.json').read_text());assert controls['sha']==source_sha;assert controls['expected_controls']==9;assert controls['stop']=='index8 native-key'
expected_names=['sync','leverage','update','raw','balance','position','cache','sync-emit','native-key'];expected_assertions=['ENTRY-sync','ENTRY-leverage','ENTRY-update','ENTRY-raw','ENTRY-balance','ENTRY-position','ENTRY-CACHE-WRITE','ENTRY-SYNC-ERROR-EMIT','PRICE-NATIVE-SCALAR-KEY'];assert [c['name'] for c in controls['controls']]==expected_names
for c,want in zip(controls['controls'],expected_assertions):
 assert c['assertion']==want and c['mutated']==101 and c['restored']==0 and c['runtime_discrimination'] and c['exact_source_restored']
 log=(gated/(c['name']+'-mutated.log')).read_text();assert 'running 1 test' in log and want in log and 'could not compile' not in log
cohort=json.loads((out/'cohort.json').read_text());assert cohort['row_floor']==25 and cohort['stop']=='BitrueExchange.fetchBalance';assert cohort['dispositions']=={'rationale-to-write':16,'code-to-fix':6,'already-supported':3};assert len(cohort['rows'])==25
counts=collections.Counter();changed=[]
for p in pathlib.Path('sdk-rs/parity/manifest/exchanges/bitrue').glob('*.toml'):
 old=tomllib.loads(subprocess.check_output(['git','show',base+':'+str(p)],text=True));new=tomllib.loads(subprocess.check_output(['git','show',source_sha+':'+str(p)],text=True));assert old['file']==new['file'];assert len(old['symbol'])==len(new['symbol'])
 for a,b in zip(old['symbol'],new['symbol']):
  assert {k:v for k,v in a.items() if k!='note'}=={k:v for k,v in b.items() if k!='note'};counts[b['status']]+=1
  if a['note']!=b['note']:changed.append(b['ts'])
assert counts=={'ported':82,'divergent':25} and len(changed)==25 and sum(counts.values())==107
source_identity=subprocess.run(['git','diff','--exit-code',source_sha,head,'--','sdk-rs/safe-cex-venue-bitrue/src','sdk-rs/safe-cex-venue-bitrue/tests'],capture_output=True,text=True);assert source_identity.returncode==0
reference_identity=subprocess.run(['git','diff','--exit-code',base,head,'--','packages/safe-cex/src/exchanges/bitrue'],capture_output=True,text=True);assert reference_identity.returncode==0
results=[tuple(map(int,re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored',s).groups())) for s in (gated/'tests.log').read_text().splitlines() if 'test result:' in s];assert tuple(map(sum,zip(*results)))==(60,0,0)
receipt={'verified_at_head':head,'gate_sha':source_sha,'integration_parent':base,'manifest_base_sha':base,'manifest_gate_sha':source_sha,'verification_exit':0,'tomllib':{'ported':82,'divergent':25,'pending':0},'independent_expected_total':107,'changed_notes':25,'status_target_layer_changes':0,'cohort_stop':'iteration25 fetchBalance','dispositions':cohort['dispositions'],'scoped_gate_exits':{r['name']:r['exit'] for r in records[:5]},'tests':{'passed':60,'failed':0,'ignored':0},'controls':{'named_runtime_failures':9,'restored_passes':9,'compiler_blocked':0,'survivors':0,'stop':'index8 native-key'},'source_tests_identity_exit':source_identity.returncode,'typescript_identity_exit':reference_identity.returncode}
# Reverification reads the frozen gate manifests and never rewrites the historical receipt.
print(json.dumps(receipt,indent=2))
