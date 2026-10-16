import pathlib,subprocess,json,tomllib,collections,os,re,tempfile
out=pathlib.Path(__import__('sys').argv[1]);out.mkdir(exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();parent=subprocess.check_output(['git','rev-parse','157f83240a'],text=True).strip();env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');results=[]
for name,cmd in [('extractor',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check']),('coverage',['node','sdk-rs/safe-cex-venue-blofin/tests/public_stream_core_scope.cjs'])]:
 r=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/(name+'.log')).write_text(r.stdout);results.append(dict(sha=sha,name=name,command=cmd,exit=r.returncode));(out/'source.json').write_text(json.dumps(dict(sha=sha,results=results),indent=2)+'\n');print(name,r.returncode,flush=True);assert r.returncode==0,name
 if name=='extractor':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in r.stdout,'extractor text'
 else:
  rows=json.loads(r.stdout);expected={'blofin.ws-public.ts':18};assert set(rows)==set(expected),'coverage files'
  for file,count in expected.items():assert len(rows[file])==count,'coverage '+file
before=collections.Counter();after=collections.Counter();moved=[];paths=sorted(pathlib.Path('sdk-rs/parity/manifest/exchanges/blofin').glob('*.toml'));assert len(paths)==7,'manifest floor'
for p in paths:
 old_rows=tomllib.loads(subprocess.check_output(['git','show',parent+':'+str(p)],text=True))['symbol'];now=tomllib.loads(p.read_text())['symbol'];before.update(r['status'] for r in old_rows);after.update(r['status'] for r in now);old={r['ts']:r for r in old_rows}
 for r in now:
  if old[r['ts']]['status']!=r['status']:assert old[r['ts']]['status']=='pending' and r['rs'],'real moved target '+r['ts'];moved.append(dict(ts=r['ts'],status=r['status'],rs=r['rs']))
assert len(moved)==10,'moved rows';assert after=={'ported':28,'divergent':72,'pending':25},'after triple';assert before=={'ported':19,'divergent':71,'pending':35},'before triple';(out/'census.json').write_text(json.dumps(dict(sha=sha,parent=parent,before=dict(before),after=dict(after),moved=moved,moved_rows=10,stopped_after_files=7),indent=2)+'\n');print('census10 moved28/72/25',flush=True)
sdk=pathlib.Path('sdk-rs').resolve()
with tempfile.TemporaryDirectory(prefix='blofin-public-stream-parity-',dir=sdk/'target') as tmp:
 p=pathlib.Path(tmp);(p/'Cargo.toml').write_text(re.sub(r'members = \[.*?\]','members = ["safe-cex-venue-blofin"]',(sdk/'Cargo.toml').read_text(),count=1,flags=re.S))
 for folder in sdk.iterdir():
  if folder.is_dir() and (folder/'Cargo.toml').exists():(p/folder.name).symlink_to(folder,target_is_directory=True)
 (p/'exempt.toml').write_text('');cmd=[str(sdk/'target/debug/parity-check'),'--workspace',str(p),'--manifest',str(sdk/'parity/manifest/exchanges/blofin'),'--exempt',str(p/'exempt.toml')];r=subprocess.run(cmd,capture_output=True,text=True);output=r.stdout+r.stderr;(out/'rust-parity.log').write_text(output);results.append(dict(sha=sha,name='rust-parity',exit=r.returncode,command=cmd,binary_built_this_slice=False,stdout_bytes=len(r.stdout.encode()),stderr_bytes=len(r.stderr.encode())));assert r.returncode==1 and output.count('invention:')==3 and all('position_adapter.rs::' in l for l in output.splitlines()),'only3 inherited Rust findings';print('Rust parity1 only3 inherited findings; stdout/stderr',len(r.stdout),len(r.stderr),flush=True)
cmd=['git','diff','--exit-code',parent,sha,'--','sdk-rs/safe-cex-venue-blofin/src/position_adapter.rs','sdk-rs/safe-cex-transport/src/tealstreet.rs'];r=subprocess.run(cmd,capture_output=True,text=True);assert r.returncode==0,'inherited file preservation';(out/'inherited-preservation.json').write_text(json.dumps(dict(sha=sha,parent=parent,command=cmd,exit=r.returncode),indent=2)+'\n');(out/'source.json').write_text(json.dumps(dict(sha=sha,results=results,source_floor_files=1,source_floor_declarations=18),indent=2)+'\n')
