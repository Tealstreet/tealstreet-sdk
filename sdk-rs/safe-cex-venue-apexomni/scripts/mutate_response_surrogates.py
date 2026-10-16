"""Prove response-order and observed-surrogate assertions with compiling controls."""
from pathlib import Path
import hashlib,json,os,subprocess,time

root=Path(__file__).resolve().parents[3]
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode==0
out=root/'sdk-rs/target/apexomni-response-surrogate-controls'/sha
out.mkdir(parents=True,exist_ok=True)
receipt={'sha':sha,'attempts':[],'survivors':[],'invalid_controls':[]}
base=[os.environ.get('CARGO','cargo'),'test','--locked','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--jobs','2']
all_targets=['--test','exchange','--test','pure','--no-fail-fast']
def run(name,args,extra=None,expected=0,labels=()):
 env=os.environ.copy();env.update(extra or {})
 start=time.monotonic();p=subprocess.run(base+args+['--','--nocapture'],cwd=root,env=env,capture_output=True,text=True)
 text=p.stdout+p.stderr;(out/(name+'.log')).write_text(text)
 row={'name':name,'sha':sha,'exit':p.returncode,'seconds':round(time.monotonic()-start,2),'expected_exit':expected,'required_labels':list(labels),'observed_labels':[label for label in labels if label in text],'log':name+'.log'}
 receipt['attempts'].append(row)
 if expected==101:
  if p.returncode==0:receipt['survivors'].append({'name':name,'labels':list(labels)})
  elif p.returncode!=101 or 'test result: FAILED.' not in text:receipt['invalid_controls'].append(name)
  else:
   for label in labels:
    if label not in text:receipt['survivors'].append({'name':name,'missing_label':label})
 else:assert p.returncode==expected,text
 (out/'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
 print(name,'exit',p.returncode,'labels',len(row['observed_labels']),'/',len(labels),flush=True)
 return text

paths={name:root/'sdk-rs/safe-cex-venue-apexomni/src'/name for name in ['api.rs','exchange_utils.rs','exchange.rs']}
saved={name:p.read_text() for name,p in paths.items()}
changed=dict(saved)
def replace(name,before,after):
 assert changed[name].count(before)==1,(name,before)
 changed[name]=changed[name].replace(before,after)
changed['api.rs']=saved['api.rs'].replace('serialize(payload, false)','serialize(payload, true)')
legacy='''pub fn serialize_apex_payload_legacy(payload: &JsJsonValue) -> Result<String, String> {
    serialize(payload, true)
}'''
assert changed['api.rs'].count(legacy)==1
changed['api.rs']=changed['api.rs'].replace(legacy,legacy.replace('true','false'))
replace('exchange_utils.rs','apexomni-{account_id}-{timestamp}-{}','apexomni-{account_id}-{timestamp}_{}')
replace('exchange.rs','property(property(&response, "data")?, "id")?', 'get(get(&response, "data"), "id")')
replace('exchange.rs','let account_positions = property(account, "positions")?;', 'let account_positions = get(account, "positions");')
replace('exchange.rs','''        let mut filtered_orders = Vec::new();''','''        let mut orders = Vec::new();''')
replace('exchange.rs','''                filtered_orders.push(row);''','''                orders.extend(self.map_order(row)?);''')
replace('exchange.rs','''        let mut orders = Vec::new();
        for row in filtered_orders {
            orders.extend(self.map_order(row)?);
        }
        Ok(orders)''','''        Ok(orders)''')
receipt['native_patch_sha256']=hashlib.sha256(''.join(changed[n] for n in sorted(changed)).encode()).hexdigest()
labels=[
 'APEX-SURROGATE serializeApexPayload/empty native result',
 'APEX-SURROGATE serializeApexPayloadLegacy/empty native result',
 'APEX-SURROGATE generateApeXRandomClientId/pair native result',
 'APEX-ENVELOPE getAccount/null-root native expected rejection',
 'APEX-ENVELOPE fetchPositions/null-data native expected rejection',
 'APEX-ENVELOPE fetchOrders/null-after-missing-market native operation order',
]
try:
 for n,p in paths.items():p.write_text(changed[n])
 run('six-native-mutations',all_targets,expected=101,labels=labels)
finally:
 for n,p in paths.items():p.write_text(saved[n])
run('six-native-restored',all_targets)
p=paths['exchange.rs'];original=saved['exchange.rs']
before='''            let cancelled = if let J::String(id) = id {
                self.recently_cancelled_orders.borrow().contains_key(id)
            } else {
                false
            };'''
assert original.count(before)==1
try:
 p.write_text(original.replace(before,'''            let cancelled = self.recently_cancelled_orders.borrow().contains_key(&text(id)?);'''))
 run('map-key-coercion-mutation',['--test','exchange','order_envelopes_match_actual_source'],expected=101,labels=['native closed Order.id must visibly refuse numeric ids'])
finally:p.write_text(original)
run('map-key-coercion-restored',['--test','exchange','order_envelopes_match_actual_source'])
before='            .retain(|_, timestamp| {\n                let age = now - *timestamp;\n                age.is_nan() || age <= 10_000.0\n            });'
assert original.count(before)==1
try:
 p.write_text(original.replace(before, '            .retain(|_, timestamp| now - *timestamp <= 10_000.0);'))
 run('nan-cleanup-mutation',['--test','exchange','order_envelopes_match_actual_source'],expected=101,labels=['APEX-ENVELOPE fetchOrders/cache-nan native value'])
finally:p.write_text(original)
run('nan-cleanup-restored',['--test','exchange','order_envelopes_match_actual_source'])
for name,test in [
 ('serializeApexPayload','primary_surrogate_observability'),
 ('serializeApexPayloadLegacy','legacy_surrogate_observability'),
 ('generateApeXRandomClientId','client_id_surrogate_observability'),
]:
 run('source-'+name,['--test','pure',test],{'SAFE_CEX_APEX_SURROGATE_CONTROL':name},expected=101,labels=[f'APEX-SURROGATE {name}/high source observable result'])
run('source-controls-restored',['--test','pure','surrogate_observability'])
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode==0
assert not receipt['survivors'] and not receipt['invalid_controls'],receipt
print('CONTROLS COMPLETE',sha,out.relative_to(root),flush=True)
