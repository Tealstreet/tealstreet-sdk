"""Fixed protocol docs/plans/RUST_PERF_CROSSOVER19.md; no direction-based retries."""
import hashlib,json,math,re,subprocess,sys,time,os,shutil
from pathlib import Path
MANIFEST=Path('/tmp/perf-crossover19b-inputs.json')
REPORT=Path('/tmp/perf-crossover19.json')
EXPECTED_MANIFEST_SHA='749c690f7c5e78bfeeacac79bca7fcc9d118086e036bc3b457fb031af576cc0b'
ROSTER=['binance','bybit','okx','cryptocom','weex','bingx','mexc','blofin','backpack','bitget','hyperliquid','bitrue','woo','lighter','orderly','bybitinverse','bybitspot','bybitdemo','hyrotrader']
BOOKS={'binance':'BTCUSDT,ETHUSDT,SOLUSDT','cryptocom':'BTCUSD-PERP,ETHUSD-PERP','backpack':'SOL_USDC_PERP,BTC_USDC_PERP','hyperliquid':'BTC-PERP,ETH-PERP','bitrue':'E-BTC-USDT,E-ETH-USDT','lighter':'ETH,BTC','orderly':'BTCUSDC,ETHUSDC','bybitinverse':'BTCUSD,ETHUSD'}
PERF_ERRORS={'Rust main-thread CPU is not below TS','Rust steady-state CPU is not below TS','Rust retained memory is not below TS'}
KEYS=['cpuMs','steadyCpuMs','processCpuMs','retainedBytes','wasmBytes']
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def memory_errors(m):
 errors=[]
 for k in ['heapBeforeBytes','heapUsedBytes','buffersBefore','buffersAfter','wasmBytes','retainedBytes','heapRetainedBytes','bufferRetainedBytes',*KEYS]:
  x=m.get(k)
  if type(x) not in [int,float] or not math.isfinite(x): errors.append('invalid '+k)
  elif k not in ['heapRetainedBytes','bufferRetainedBytes','retainedBytes'] and x<0: errors.append('negative '+k)
 for key,expected in [('heapRetainedBytes',lambda:m['heapUsedBytes']-m['heapBeforeBytes']),('bufferRetainedBytes',lambda:m['buffersAfter']-m['buffersBefore']),('retainedBytes',lambda:m['heapRetainedBytes']+m['bufferRetainedBytes']+m['wasmBytes'])]:
  try:
   if m[key]!=expected(): errors.append('inconsistent '+key)
  except (KeyError,TypeError): errors.append('missing arithmetic '+key)
 return sorted(set(errors))
def audit_errors(sample):
 a=sample.get('audit',{})
 for key in ['unserved','invalidSignatures','unusedWrites','missingWrites']:
  value=a.get(key)
  if type(value) not in [int,float] or not math.isfinite(value) or value!=0:return True
 return a.get('protocolErrors')!=[] or a.get('hasAccount') is not False or sample['markets']<=0 or sample['tickers']<=0
def classify(row,runs):
 text=row['logText']; outputs=row['outputs']; exit_code=row['exit']
 if any(audit_errors(sample) for samples in outputs.values() for sample in samples):return 'fatal'
 # A sampler/wall deadline, signal, truncated log or unexpected execution failure
 # is operationally fatal, even when a memory phrase is also printed.
 if row.get('executionError'): return 'fatal'
 issues=[]
 match=re.search(r'^FAIL \((\d+)\)\n',text,re.M)
 if match:
  issues=[line[2:] for line in text[match.end():].splitlines() if line.startswith('  ')]
  if int(match[1])!=len(issues): return 'fatal'
 settled='Backing-buffer memory did not settle' in text
 settled_error=r'Backing-buffer memory did not settle \(samples=\d+, transitions=\d+, lastArrayBuffers=\d+, elapsedMs=\d+(?:\.\d+)?\)'
 if settled:
  if exit_code!=1 or match: return 'fatal'
  error_messages=re.findall(r'^(?:Error|[A-Za-z]+Error)(?: \[[-A-Z0-9_]+\])?: (.*)$',text,re.M)
  if not error_messages or not all(re.fullmatch(settled_error,msg) or re.fullmatch(r'(?:ts|rust) replay failed: *',msg) for msg in error_messages): return 'fatal'
  if not any(re.fullmatch(settled_error,msg) for msg in error_messages): return 'fatal'
  if re.search(r'Replay exceeded|deadline|SIG[A-Z]+',text): return 'fatal'
  # TS runs first. A settled-memory exception permits only its complete valid
  # output prefix, never a later Rust file or a partial prior TS metric.
  if set(outputs) not in [set(),{'ts'}]: return 'fatal'
  if outputs and (len(outputs['ts'])!=runs or any(memory_errors(sample['metrics']) or sample['metrics']['retainedBytes']<0 for sample in outputs['ts'])): return 'fatal'
  return 'quality' 
 if set(outputs)!=set(['ts','rust']) or any(len(v)!=runs for v in outputs.values()): return 'fatal'
 negative=False
 for samples in outputs.values():
  for sample in samples:
   m=sample['metrics']
   if memory_errors(m): return 'fatal'
   negative |= m['retainedBytes']<0
 # Each non-performance gate line must be precisely a finite negative total.
 quality_lines=[s for s in issues if re.fullmatch(r'(?:ts|rust): invalid retainedBytes \(-\d+(?:\.\d+)?\); baseline is invalid',s)]
 if any(s not in PERF_ERRORS and s not in quality_lines for s in issues): return 'fatal'
 if negative:
  if not quality_lines or exit_code!=1: return 'fatal'
  return 'quality'
 if quality_lines: return 'fatal'
 if exit_code==0 and (match or not re.search(r'^PASS$',text,re.M)): return 'fatal'
 if exit_code!=0 and (exit_code!=1 or not match or not issues): return 'fatal'
 return 'valid'
def median(xs): return sorted(xs)[len(xs)//2]
def objective_errors(venue,summary):
 errors=[]
 for key in ['cpuRatio','steadyRatio','memoryRatio']:
  if not summary['after'][key]>1:errors.append([venue,'after',key,'aggregate threshold'])
 for key in ['cpuRatio','memoryRatio']:
  if not summary['after'][key]>summary['before'][key]:errors.append([venue,'after',key,'not wider'])
 return errors
def summarize(rows,side):
 result={}
 for impl in ['ts','rust']:
  samples=[sample['metrics'] for row in rows for sample in row['sides'][side]['outputs'][impl]]
  result[impl]={key:median([m[key] for m in samples]) for key in KEYS}
  result[impl]['cpuRangeMs']=[min(m['cpuMs'] for m in samples),max(m['cpuMs'] for m in samples)]
 if any(result['rust'][key]<=0 for key in ['cpuMs','steadyCpuMs','retainedBytes']):raise RuntimeError('zero denominator in pooled Rust metrics: '+repr(result))
 return {'metrics':result,'cpuRatio':result['ts']['cpuMs']/result['rust']['cpuMs'],'steadyRatio':result['ts']['steadyCpuMs']/result['rust']['steadyCpuMs'],'memoryRatio':result['ts']['retainedBytes']/result['rust']['retainedBytes']}
def main():
 manifest=json.loads(MANIFEST.read_text()); manifest_sha=sha(MANIFEST)
 roots={k:Path(v) for k,v in manifest['roots'].items()}
 node=manifest['node']; executable=Path(node['executable'])
 env=dict(os.environ)
 env['PATH']=str(executable.parent)+os.pathsep+env.get('PATH','')
 corepack=shutil.which('corepack',path=env['PATH'])
 preflight_command=[corepack,'yarn','-s','node','-p','process.execPath']
 def fingerprint():
  assert sha(MANIFEST)==manifest_sha,'manifest changed'
  counts={}
  physical={}
  for side,root in roots.items():
   inputs=[*manifest['shared'],*manifest[side]]
   for entry in inputs:
    path=root/entry['relativePath'];real=path.resolve()
    if real not in physical:physical[real]=sha(path)
    actual=physical[real]
    assert actual==entry['sha256'],{'side':side,'path':entry['relativePath'],'expected':entry['sha256'],'actual':actual}
   counts[side]=len(inputs)
  node=manifest['node'];assert sha(Path(node['executable']))==node['sha256'],'Node changed'
  actual=subprocess.check_output([node['executable'],'--version'],text=True,timeout=10).strip();assert actual==node['version']
  return {'manifestSha256':manifest_sha,'checkedFiles':counts}
 report={'driverSha256':sha(Path(__file__)),'protocol':'docs/plans/RUST_PERF_CROSSOVER19.md production ecca589fb3 / shared helpers71ad0586e0','manifest':manifest,'status':'running','official':[],'attempts':[],'summary':{}}
 with REPORT.open('x') as f:json.dump(report,f,indent=2)
 def persist(): REPORT.write_text(json.dumps(report,indent=2))
 def run(venue,side,label,runs,attach):
  cwd=roots[side];log=Path(f'/tmp/perf-crossover19-{label}-{side}-{venue}.log')
  cmd=['corepack','yarn','-s','rust:gate','--venue',venue,'--tape',f'sdk-rs/harness/fixtures/{venue}-public.jsonl.gz','--books',BOOKS.get(venue,'BTCUSDT,ETHUSDT'),'--runs',str(runs)]
  out=cwd/'sdk-rs/harness/out';existing=set(out.glob(venue+'-*'))
  row={'venue':venue,'side':side,'command':cmd,'log':str(log),'startedAtUnix':time.time(),'outputs':{},'launcher':corepack,'node':node}
  attach(row);persist()
  try:
   with log.open('x') as f:
    result=subprocess.run([corepack,*cmd[1:]],cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=900)
    row['exit']=result.returncode
  except Exception as e:row.update(exit=None,executionError=repr(e))
  row['elapsedSeconds']=time.time()-row['startedAtUnix'];persist()
  try:
   row['logText']=log.read_text() if log.exists() else ''
   created=set(out.glob(venue+'-*'))-existing
   row['createdOutputDirectories']=[str(d) for d in created];persist()
   if len(created)==1:
    directory=created.pop()
    for impl in ['ts','rust']:
     samples=[]
     for n in range(runs):
      path=directory/f'{impl}-{n}.json'
      if path.exists():
       row['readingOutput']=str(path);persist()
       data=json.loads(path.read_text());samples.append({'path':str(path),'sha256':sha(path),'metrics':data['metrics'],'frames':data['frames'],'markets':len(data['markets']),'tickers':len(data['tickers']),'books':len(data['books']),'audit':{**{key:data.get(key) for key in ['unserved','invalidSignatures','unusedWrites','missingWrites','protocolErrors']},'hasAccount':data.get('account') is not None}})
     if samples:row['outputs'][impl]=samples;persist()
   elif created:row['executionError']='unexpected multiple output directories'
  except Exception as e:row['executionError']=repr(e)
  row.setdefault('logText','')
  try:row['classification']=classify(row,runs)
  except Exception as e:row.update(executionError=repr(e),classification='fatal')
  persist()
  return row
 try:
  assert manifest_sha==EXPECTED_MANIFEST_SHA,'manifest differs from reviewed frozen hash'
  report['launcherPreflight']={'command':preflight_command,'timeoutSeconds':30};persist()
  assert corepack,'corepack missing'
  launched=subprocess.check_output(preflight_command,cwd=roots['after'],env=env,text=True,timeout=30).strip()
  report['launcherPreflight']['actualExecutable']=launched;persist()
  assert Path(launched).resolve()==executable.resolve(),('launcher Node differs',launched,node)
  report['initialFingerprint']=fingerprint();persist()
  for venue in ROSTER:
   row=run(venue,'after','official',3,report['official'].append);persist()
   report['lastFingerprint']=fingerprint();persist()
   print('official',venue,row['exit'],row['classification'],flush=True)
   if row['exit']!=0 or row['classification']!='valid':raise RuntimeError('independent official gate failed '+venue)
  valid={v:[] for v in ROSTER};attempts={v:0 for v in ROSTER}
  for round_index in range(10):
   order=ROSTER[round_index:]+ROSTER[:round_index]
   for venue in order:
    while len(valid[venue])<=round_index:
     if attempts[venue]>=15:raise RuntimeError('quality budget exhausted '+venue)
     pair_index=len(valid[venue]);attempt=attempts[venue];attempts[venue]+=1
     sides=['before','after'] if (pair_index+ROSTER.index(venue))%2==0 else ['after','before']
     row={'venue':venue,'round':round_index,'validPairIndex':pair_index,'attempt':attempt,'order':sides,'sides':{},'fingerprintBefore':fingerprint()}
     report['attempts'].append(row);persist()
     for side in sides:
      run(venue,side,f'pair{attempt}',1,lambda result:row['sides'].__setitem__(side,result));persist()
     row['fingerprintAfter']=fingerprint()
     statuses=[s['classification'] for s in row['sides'].values()]
     row['classification']='fatal' if 'fatal' in statuses else ('quality' if 'quality' in statuses else 'valid');persist()
     print('pair',venue,attempt,sides,row['classification'],[s['exit'] for s in row['sides'].values()],flush=True)
     if row['classification']=='fatal':raise RuntimeError('fatal comparison failure '+venue)
     if row['classification']=='valid':valid[venue].append(row)
  failures=[]
  for venue,rows in valid.items():
   assert len(rows)==10 and sum(r['order'][0]=='before' for r in rows)==5
   summary={s:summarize(rows,s) for s in ['before','after']}
   summary['orderStrata']={order:{s:summarize([r for r in rows if r['order'][0]==order],s) for s in ['before','after']} for order in ['before','after']}
   summary['pairedMargins']=[]
   for r in rows:
    margins={'order':r['order']}
    for label,key in [('cpuRatio','cpuMs'),('steadyRatio','steadyCpuMs'),('memoryRatio','retainedBytes')]:
     values={s:r['sides'][s]['outputs']['ts'][0]['metrics'][key]/r['sides'][s]['outputs']['rust'][0]['metrics'][key] for s in ['before','after']}
     margins[label]={**values,'change':values['after']-values['before']}
    summary['pairedMargins'].append(margins)
   summary['thresholdStatus']={side:{key:summary[side][key]>1 for key in ['cpuRatio','steadyRatio','memoryRatio']} for side in ['before','after']}
   failures.extend(objective_errors(venue,summary))
   report['summary'][venue]=summary
  report.update(status='pass' if not failures else 'performance objective open',failures=failures);persist()
  print(json.dumps({'status':report['status'],'failures':failures,'summary':report['summary']},indent=2),flush=True)
 except BaseException as e:
  report.update(status='aborted',error=repr(e));persist();raise
if __name__=='__main__':main()
