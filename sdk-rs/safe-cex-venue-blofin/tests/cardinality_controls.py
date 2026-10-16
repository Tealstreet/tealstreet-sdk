"""Sequential baseline empty/subset production controls. Baseline survivors are recorded, not called passes."""
import atexit,hashlib,json,os,pathlib,subprocess,sys
ROOT=pathlib.Path(__file__).resolve().parents[3]
CRATE=ROOT/'sdk-rs/safe-cex-venue-blofin'
BASE='/opt/homebrew/opt/rustup/bin/cargo'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
MODE=sys.argv[1] if len(sys.argv)>1 else 'baseline'
logs=ROOT/f'sdk-rs/target/blofin-cardinality-{MODE}'
logs.mkdir(exist_ok=True)
originals={p:p.read_text() for p in (CRATE/'src').glob('*.rs')}
oracles={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (CRATE/'tests').glob('*_oracle.cjs')}
(logs/'oracle-sha256.json').write_text(json.dumps(oracles,indent=2)+'\n')
sites=[('income','income','Ok(records)','records'),('tickers_project','tickers','    Ok(tickers)\n}', 'tickers'),('tickers_fallback','tickers','Ok(store.tickers().to_vec())','store.tickers().to_vec()'),('orders','orders','Ok(result)','result'),('positions','positions','Ok(out)','out'),('inline_tpsl','inline_tpsl','Ok(result)','result')]
for name in ['FetchPositionResponse','FetchBatchLevResponse','OrdersResponse','PositionsResponse']:sites.append((name,'types',None,None))
summary=[]
support=CRATE/'tests/support/cardinality.rs'
support_original=support.read_bytes()
atexit.register(support.write_bytes,support_original)
if MODE=='bypass':
 source=support_original.decode();a=source.index('        for (path, count)');b=source.index('        this\n',a);support.write_text(source[:a]+'        let _ = corpus;\n'+source[b:])
for name,target,anchor,variable in sites:
 for count,label in [(0,'empty'),(1,'subset'),(None,'extra')]:
  path=CRATE/'src'/f'{target}.rs';original=originals[path]
  if anchor:
   assert original.count(anchor)==1,(name,anchor)
   replacement=f'Ok(({variable}).into_iter().take({count}).collect())' if count is not None else f'Ok({{ let mut rows = {variable}; if let Some(row) = rows.first().cloned() {{ rows.push(row); }} rows }})'
   if name=='tickers_project': replacement='    '+replacement+'\n}'
   mutated=original.replace(anchor,replacement)
  else:
   start=original.index(f"impl<'de> Deserialize<'de> for {name} {{")
   end=original.index('\n}',start)+2
   block=original[start:end];assert block.count('data: wire.data,')==1,name
   expr=f'wire.data.into_iter().take({count}).collect()' if name.startswith('Fetch') else f'wire.data.map(|rows| rows.into_iter().take({count}).collect())'
   if count is None:
    extra='{ let mut rows = wire.data; if let Some(row) = rows.first().cloned() { rows.push(row); } rows }'
    expr=extra if name.startswith('Fetch') else 'wire.data.map(|mut rows| { if let Some(row) = rows.first().cloned() { rows.push(row); } rows })'
   mutated=original[:start]+block.replace('data: wire.data,',f'data: {expr},')+original[end:]
  command=[BASE,'test','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-blofin','--locked','--jobs','1','--test',target,'--','--nocapture']
  try:
   path.write_text(mutated)
   run=subprocess.run(command,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
   (logs/f'{name}.{label}.mutated.log').write_text(run.stdout)
   if run.returncode==0: verdict='SURVIVED'
   elif run.returncode==101 and 'panicked at' in run.stdout and 'test result: FAILED' in run.stdout: verdict='RUNTIME_REJECTED'
   else: raise AssertionError(run.stdout)
   if MODE!='baseline': assert 'BLOFIN-CARDINALITY native length' in run.stdout,run.stdout
   summary.append({'site':name,'mutation':label,'verdict':verdict,'exit':run.returncode})
   print(name,label,verdict,run.returncode,flush=True)
  finally:path.write_text(original)
  run=subprocess.run(command,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (logs/f'{name}.{label}.restored.log').write_text(run.stdout)
  assert run.returncode==0,run.stdout
  print(name,label,'RESTORED',run.returncode,flush=True)
  (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
assert all(p.read_text()==v for p,v in originals.items()),'production_restored'
assert oracles=={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (CRATE/'tests').glob('*_oracle.cjs')},'oracles_unchanged_during_controls'
support.write_bytes(support_original)
if MODE!='baseline':assert all(s['verdict']=='RUNTIME_REJECTED' for s in summary),'surviving_cardinality_mutant'
