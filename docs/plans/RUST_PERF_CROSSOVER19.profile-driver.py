import hashlib,json,os,subprocess,time,shutil
from pathlib import Path
root=Path('/Users/sam/cs/tealstreet-sdk/.aimux/worktrees/rust-okx')
proof=Path('/tmp/perf-nineteen-profile-provenance19b.json')
manifest_path=Path('/tmp/perf-crossover19b-inputs.json')
manifest_sha='749c690f7c5e78bfeeacac79bca7fcc9d118086e036bc3b457fb031af576cc0b'
r=json.loads(proof.read_text())
assert r['status']=='build complete; no measurements' and r['attempts']==[],'refuse to replace prior profile attempts'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def persist():proof.write_text(json.dumps(r,indent=2))
def verify():
 assert sha(manifest_path)==manifest_sha,'manifest changed'
 manifest=json.loads(manifest_path.read_text());physical={}
 def check(path,expected):
  f=path.resolve()
  if f not in physical:physical[f]=sha(path)
  assert physical[f]==expected,str(path)
 for key in ['rustSourceHashes','helperHashes']:
  for p,h in r[key].items():check(root/p,h)
 for p,v in r['profileArtifacts'].items():check(root/p,v['sha256'])
 for p in [*manifest['shared'],*manifest['after']]:check(root/p['relativePath'],p['sha256'])
 node=Path(manifest['node']['executable']);check(node,manifest['node']['sha256'])
 assert subprocess.check_output([str(node),'--version'],text=True,timeout=10).strip()==manifest['node']['version'],'Node version changed'
 return manifest
r.update(status='preflight',inputManifestSha256=manifest_sha,driverSha256=sha(Path(__file__)),startedAtUnix=time.time());persist()
try:
 manifest=verify();node=Path(manifest['node']['executable']);env=dict(os.environ);env['PATH']=str(node.parent)+os.pathsep+env['PATH'];env['ESBUILD_WORKER_THREADS']='0'
 corepack=shutil.which('corepack',path=env['PATH']);assert corepack
 preflight=[corepack,'yarn','-s','node','-p','process.execPath']
 r['launcherPreflight']={'command':preflight,'timeoutSeconds':30};persist()
 actual=subprocess.check_output(preflight,cwd=root,env=env,text=True,timeout=30).strip()
 r['launcherPreflight']['actualExecutable']=actual;persist();assert Path(actual).resolve()==node.resolve(),'Corepack selected different Node'
 r['status']='profiling';persist()
 for venue in ['okx','bybit']:
  for mode in ['single','batch']:
   cmd=[corepack,'yarn','-s','rust:profile','--venue',venue,'--delivery',mode,'--runs','3','--probe-rounds','3']
   out=Path(f'/tmp/perf-nineteen-profile-{venue}-{mode}.json');log=Path(f'/tmp/perf-nineteen-profile-{venue}-{mode}.stderr.log')
   row={'venue':venue,'delivery':mode,'command':cmd,'output':str(out),'stderr':str(log),'startedAtUnix':time.time(),'exit':None};r['attempts'].append(row);persist()
   try:
    with out.open('x') as f,log.open('x') as err:
     run=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=err,timeout=600)
    row['exit']=run.returncode
    assert run.returncode==0,(run.returncode,log.read_text())
    data=json.loads(out.read_text());assert data['runs']==data['rounds']==3 and len(data['replay']['samples'])==3
    verify();row['status']='pass';print(venue,mode,'PASS',flush=True)
   except BaseException as e:
    row.update(status='failed',error=repr(e));raise
   finally:
    row['elapsedSeconds']=time.time()-row['startedAtUnix']
    for label,path in [('output',out),('stderr',log)]:
     if path.is_file():row[label+'Sha256']=sha(path)
    persist()
 r['status']='pass';persist();print('PASS four configurations',flush=True)
except BaseException as e:
 r.update(status='failed',error=repr(e),elapsedSeconds=time.time()-r['startedAtUnix']);persist();raise
