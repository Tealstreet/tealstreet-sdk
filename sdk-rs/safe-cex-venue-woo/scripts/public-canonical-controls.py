import hashlib,json,os,pathlib,subprocess
ROOT=pathlib.Path(__file__).resolve().parents[3]
SOURCE=ROOT/'sdk-rs/safe-cex-venue-woo/src/public_canonical.rs'
ARTIFACTS=ROOT/'.p6-review'
ARTIFACTS.mkdir(parents=True, exist_ok=True)
original=SOURCE.read_bytes();text=original.decode()
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
env=dict(os.environ)
cargo=[os.environ.get('CARGO', 'cargo'),'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','public_canonical_semantics','--jobs','2']
controls=[
 ('projected-v3-symmetric-fields','decoded_native_projector_fields_and_getter_order','native projected field fundingRate',text.replace('("fundingRate", "estimatedFundingRate")','("fundingRate", "volume24h")').replace('("volume", "volume24h")','("volume", "estimatedFundingRate")')),
 ('sort-comparator-reversed','native_list_stable_order_copy_and_prepared_fields','native sorted stable IDs',text.replace('argument(&ops, &args, 0), "symbol"','argument(&ops, &args, SWAP), "symbol"').replace('argument(&ops, &args, 1), "symbol"','argument(&ops, &args, 0), "symbol"').replace('argument(&ops, &args, SWAP), "symbol"','argument(&ops, &args, 1), "symbol"')),
 ('locale-lookup-after-rhs','native_current_comparator_member_and_actual_sort_return','native captured localeCompare member',text.replace('let f = get(h, &receiver, key)?;\n    (h.invoke)(f, receiver, args()?)','let args = args()?;\n    let f = get(h, &receiver, key)?;\n    (h.invoke)(f, receiver, args)')),
 ('stringify-wrong-value','native_stringify_undefined_numbers_utf16_and_error','native stringify undefined result',text.replace('method(h, (h.global)("JSON")?, "stringify", || Ok(vec![value]))','method(h, (h.global)("JSON")?, "stringify", || { let _ = value; Ok(vec![(h.object)(vec![])]) })',1)),
]
records=[]
def run(name,phase,test):
 cmd=cargo+[test,'--','--exact'];p=subprocess.run(cmd,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (ARTIFACTS/f'canonical-{name}-{phase}.log').write_text(p.stdout)
 return {'command':cmd,'exit':p.returncode,'output_excerpt':'\n'.join(p.stdout.splitlines()[-28:])}
try:
 for name,test,assertion,mutant in controls:
  assert mutant!=text,('mutation applied',name)
  SOURCE.write_text(mutant);red=run(name,'red',test)
  SOURCE.write_bytes(original);restored=run(name,'restored',test)
  caught=red['exit']==101 and assertion in red['output_excerpt'] and 'test result: FAILED' in red['output_excerpt'] and restored['exit']==0
  records.append({'name':name,'gate_sha':sha,'overlay':'uncommitted production mutation','test':test,'named_assertion':assertion,'red':red,'restored':restored,'caught':caught})
  print(json.dumps({'name':name,'red':red['exit'],'restored':restored['exit'],'caught':caught}),flush=True)
  assert caught,('SURVIVOR OR INVALID CONTROL',name)
finally:
 SOURCE.write_bytes(original)
 (ARTIFACTS/'canonical-controls.json').write_text(json.dumps({'sha':sha,'source_sha256':hashlib.sha256(original).hexdigest(),'controls':records,'survivors':[r['name'] for r in records if not r['caught']],'floor':'four listed compiled controls; not exhaustive'},indent=2)+'\n')
