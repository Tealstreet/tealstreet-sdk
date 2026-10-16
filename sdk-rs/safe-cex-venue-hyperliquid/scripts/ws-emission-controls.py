#!/usr/bin/env python3
"""One grouped production control plus regressions for concrete oracle defects."""
import argparse,json,re,subprocess,tempfile
from pathlib import Path
CRATE=Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1]
NAMES=['public.sendWithDiagnostics','public.emitReceiveDiagnostics','private.sendWithDiagnostics','private.emitReceiveDiagnostics','private.subscribeSystemTopics','private.subscribeAllActiveAssetData','spot_public.subscribePublicTopics','spot_public.subscribeUserTopics','spot_private.subscribeSystemTopics','spot_private.subscribeCoins']
CARGO=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid','--test','ws_emission','--jobs','2']
def replace(s,old,new,count=1):
 assert s.count(old)==count,(old,s.count(old));return s.replace(old,new)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);args=p.parse_args();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 subprocess.run(['git','diff','--exit-code','HEAD','--',str(CRATE/'src'),str(CRATE/'tests'),str(CRATE/'scripts')],cwd=ROOT,check=True)
 source=CRATE/'src/ws_emission.rs';original=source.read_bytes();oracle=CRATE/'scripts/ws-emission-oracle.cjs';original_oracle=oracle.read_bytes();logs=Path(tempfile.mkdtemp(prefix='hyperliquid-ws-emission-controls-'))
 def run(label,command):
  r=subprocess.run(command,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text(r.stdout);print(label+':exit'+str(r.returncode),flush=True);return r
 try:
  s=original.decode();s=replace(s,'DiagnosticScope::Public => "public"','DiagnosticScope::Public => "CONTROL_PUBLIC"');s=replace(s,'DiagnosticScope::Private => "private"','DiagnosticScope::Private => "CONTROL_PRIVATE"');s=replace(s,'let subscribe = literal(h, "subscribe");','let subscribe = literal(h, "CONTROL_SUBSCRIBE");',2)
  source.write_text(s);mutation=run('grouped-ten-production',CARGO)
 finally:source.write_bytes(original)
 restored=run('restored-native',CARGO);observed=[n for n in NAMES if re.search(r'failed: EMISSION native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b',mutation.stdout)]
 controls=[]
 main=['node',str(oracle)];protocol=['node',str(CRATE/'scripts/ws-emission-protocol.cjs')]
 fixes=[
  ('getv-boxing',"if(object==null)throw new TypeError('nullish property access');return Reflect.get(Object(object),name,object);",'return Reflect.get(object,name);',main,'CORE_ACTIVE_BASELINE'),
  ('undefined-throw-tag','if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;','if(tags.get(fn)===g.throwCall)throw g.error;',main,'CORE_ACTIVE_BASELINE'),
  ('iterator-value-once','return {done,value};','return step;',main,'ITERATOR_SOURCE_SINGLE_READ'),
  ('iterator-next-cache','Reflect.apply(nextMethod,real,[])','Reflect.apply(real.next,real,[])',protocol,'ITERATOR_NEXT_CACHE'),
  ('iterator-result-object',"if(step===null||(typeof step!=='object'&&typeof step!=='function'))throw new TypeError('iterator result must be an object');",'',protocol,'ITERATOR_RESULT_OBJECT'),
  ('iterator-return-cache','const close=real.return;','const close=real.return?real.return:undefined;',protocol,'ITERATOR_RETURN_CACHE'),
 ]
 for label,old,new,command,named in fixes:
  try:oracle.write_text(replace(original_oracle.decode(),old,new));negative=run(label,command)
  finally:oracle.write_bytes(original_oracle)
  positive=run(label+'-restored',command)
  # Keep full negative output; restored main emits a large JSON corpus, so retain
  # its independent floor/result summary instead of duplicating the corpus.
  summary=positive.stdout if command==protocol else {'cases':len(json.loads(positive.stdout)['cases']),'iteration_stop':json.loads(positive.stdout)['iterationStop']}
  controls.append({'name':label,'command':command,'negative_exit':negative.returncode,'named_assertion':named,'negative_output':negative.stdout,'restored_exit':positive.returncode,'restored_output_or_summary':summary})
 report={'measured_sha':sha,'cargo_command':CARGO,'mutation_exit':mutation.returncode,'required_named_native_assertions':NAMES,'observed_named_native_assertions':observed,'mutation_output':mutation.stdout,'restored_exit':restored.returncode,'restored_output':restored.stdout,'bytes_restored':source.read_bytes()==original and oracle.read_bytes()==original_oracle,'oracle_regressions':controls,'logs':str(logs),'iteration_stop':'one grouped production mutation across ten emission roots; six named oracle-regression controls for concrete precommit defects, not an old-row matrix'}
 args.output.write_text(json.dumps(report,indent=2)+'\n')
 assert mutation.returncode==101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout,mutation.stdout
 assert observed==NAMES,'survivors:'+repr(set(NAMES)-set(observed))
 for control in controls:
  assert control['negative_exit']==1 and control['named_assertion'] in control['negative_output'],control
  assert control['restored_exit']==0,control
 assert restored.returncode==0 and '1 passed; 0 failed' in restored.stdout,restored.stdout
 assert report['bytes_restored'];print('all ten native assertions and six oracle regression guards discriminate; restored gates passed',flush=True)
