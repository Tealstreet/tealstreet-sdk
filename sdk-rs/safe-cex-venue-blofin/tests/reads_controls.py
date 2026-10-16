"""Sequential production/source controls; one Node corpus per scoped Rust run."""
import pathlib, subprocess, json, re
ROOT=pathlib.Path(__file__).resolve().parents[3]
P=ROOT/'sdk-rs/safe-cex-venue-blofin/src/reads.rs'
T=ROOT/'sdk-rs/safe-cex-venue-blofin/tests/reads.rs'
J=ROOT/'sdk-rs/safe-cex-venue-blofin/tests/reads_oracle.cjs'
N=ROOT/'sdk-rs/safe-cex-num/src/numeric_ingress.rs'
original={p:p.read_bytes() for p in [P,T,J,N]}
logs=ROOT/'sdk-rs/target/blofin-reads-phase-controls';logs.mkdir(exist_ok=True)
controls=[]
def replace(path,old,new):
 s=original[path].decode();assert s.count(old)==1,(path,old,s.count(old));return s.replace(old,new).encode()
def remove_assertions(raw,labels):
 s=raw.decode()
 # Find assertion macro blocks by their terminating semicolon (no semicolons inside these assertions).
 return re.sub(r'assert(?:_eq|_ne)?!\([\s\S]*?\);',lambda m:('let _ = count;' if re.search(r'\bcount\b',m[0]) else '') if any(x in m[0] for x in labels) else m[0],s).encode()
controls += [('raw-endpoint',{P:replace(P,'ENDPOINTS["TICKERS"]','ENDPOINTS["MARKETS"]')},'READ request payload')]
controls += [('formatted-bypass',{P:replace(P,'Some(data) => format_tickers(&data, store, |message| {','Some(_data) => format_tickers(&TickerInput::from_retained(Value::Array(vec![])), store, |message| {')},'READ-CARDINALITY native outputs')]
controls += [('error-precedence',{P:replace(P,'.filter(|message| js_truthy(Some(message)))','.filter(|message| { let _ = js_truthy(Some(message)); false })')},'READ native messages')]
controls += [('emitter-discard',{P:replace(P,'emit_and_log_error(&message).map_err(ReadError::Emitter)?;','let _ = emit_and_log_error(&message);')},'READ native fields')]
for shape,op in [('empty','rows.clear();'),('subset','rows.truncate(1);'),('extra','rows.push(rows[0].clone());')]:
 raw=replace(P,'.map(TickerInput::from_retained)',f'.map(|mut value| {{ if let Some(rows)=value.as_array_mut() {{ {op} }} TickerInput::from_retained(value) }})')
 formatted=replace(P,'.map_err(ReadError::Emitter),',f'.map_err(ReadError::Emitter).map(|mut rows| {{ {op} rows }}),')
 for name,mutant in [('raw',raw),('formatted',formatted)]:
  controls.append((f'{name}-{shape}',{P:mutant},'READ-CARDINALITY native outputs'))
  controls.append((f'{name}-{shape}-source-bypassed',{P:mutant,T:remove_assertions(original[T],['READ-SOURCE'])},'READ-CARDINALITY native outputs'))
anchor='''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
consume=replace(N,anchor,'''        let _ = seed;
        for _ in self.0.by_ref() {} // Consuming EOF mutation, not trailing-element refusal.
        Ok(None)''')
controls += [('consuming-empty',{N:consume},'READ-CARDINALITY native outputs'),
 ('consuming-empty-no-length',{N:consume,T:remove_assertions(original[T],['READ-SOURCE','READ-CARDINALITY'])},'READ native fields')]
for shape,op in [('empty','cases.splice(0);'),('subset','cases.splice(1);'),('extra','cases.push(cases[0]);')]:
 controls.append((f'source-{shape}',{J:replace(J,'process.stdout.write(JSON.stringify(',op+'\n process.stdout.write(JSON.stringify(')},'READ-SOURCE corpus total'))
for shape,op in [('empty','cases[0].result.ok.splice(0);'),('subset','cases[0].result.ok.splice(1);'),('extra','cases[0].result.ok.push(cases[0].result.ok[0]);')]:
 controls.append((f'source-nested-{shape}',{J:replace(J,'process.stdout.write(JSON.stringify(',op+'\n process.stdout.write(JSON.stringify(')},'READ-SOURCE per-case output'))
for phase in ['Untransformed','Transformed']:
 code='if matches!(body,RestResponseBody::'+phase+'(_)) && value.as_str().is_some_and(|s| s.starts_with("{")) { js_from_str(value.as_str().unwrap()) } else { js_from_value_ref(value) }'
 mutant=replace(P,'js_from_value_ref(value)',code)
 controls.append((f'reparse-{phase.lower()}',{P:mutant},'READ response phase native value'))
controls += [('short-native-walk',{T:replace(T,'for fixture in 0..26 {','for fixture in 0..25 {')},'READ native comparison count')]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-blofin','--locked','--jobs','1','--test','reads','--','--nocapture'],cwd=ROOT,capture_output=True,text=True)
summary=[]
for name,changes,label in controls:
 try:
  for p,s in changes.items():p.write_bytes(s)
  r=run();out=r.stdout+r.stderr;(logs/(name+'.mutated.log')).write_text(out)
  assert r.returncode==101 and 'test result: FAILED' in out and 'panicked at' in out and label in out,(name,r.returncode,out)
  assert 'trailing array elements' not in out,(name,out)
 finally:
  for p,s in original.items():
   if p.read_bytes()!=s:p.write_bytes(s)
 # Preserve bypassed guards on restoration: their disabled state cannot cause the failure itself.
 restore_test=changes.get(T) if name.endswith('source-bypassed') or name=='consuming-empty-no-length' else None
 try:
  if restore_test is not None:T.write_bytes(restore_test)
  r=run();out=r.stdout+r.stderr;(logs/(name+'.restored.log')).write_text(out)
  assert r.returncode==0,(name,r.returncode,out)
 finally:
  if T.read_bytes()!=original[T]:T.write_bytes(original[T])
 summary.append({'control':name,'assertion':label,'mutated':101,'restored':0})
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 print(name,101,label,'RESTORED',0,flush=True)
assert all(p.read_bytes()==s for p,s in original.items())
