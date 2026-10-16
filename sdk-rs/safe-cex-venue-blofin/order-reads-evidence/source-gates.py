import pathlib,subprocess,os,json,tomllib,collections,re,tempfile
root=pathlib.Path.cwd();sdk=root/'sdk-rs';out=pathlib.Path('/tmp/blofin-order-reads-gates');out.mkdir(exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');results=[]
for name,cmd in [('extractor',['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check']),('coverage',['node','sdk-rs/safe-cex-venue-blofin/tests/coverage_scope.cjs'])]:
 p=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(name+'.log')).write_text(p.stdout);results.append(dict(name=name,sha=sha,command=cmd,exit=p.returncode));print(name,p.returncode,flush=True)
 if name=='extractor':assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in p.stdout
 else:
  declarations=json.loads(p.stdout);expected={'blofin.types.ts':20,'blofin.income.ts':3,'blofin.order-history.ts':3,'blofin.exchange.ts':66};assert set(declarations)==set(expected)
  for name,rows in declarations.items():
   actual=tomllib.loads((sdk/'parity/manifest/exchanges/blofin'/f'{name}.toml').read_text())['symbol'];assert rows==[dict(ts=r['ts'],kind=r['kind']) for r in actual];assert len(rows)==expected[name]
 print('text/source checks passed',flush=True)
(out/'source-gates.json').write_text(json.dumps(dict(sha=sha,results=results,independent_source_floor={'files':4,'declarations':92}),indent=2)+'\n')
counts=collections.Counter();before=collections.Counter();moved=[];paths=sorted((sdk/'parity/manifest/exchanges/blofin').glob('*.toml'));assert len(paths)==7
for path in paths:
 data=tomllib.loads(path.read_text());original=tomllib.loads(subprocess.check_output(['git','show','3e4ab04592:'+str(path.relative_to(root))],text=True));counts.update(r['status'] for r in data['symbol']);before.update(r['status'] for r in original['symbol']);old={r['ts']:r for r in original['symbol']}
 for row in data['symbol']:
  if row['status']!=old[row['ts']]['status']:assert old[row['ts']]['status']=='pending';moved.append(row['ts'])
assert len(moved)==8;assert counts=={'ported':14,'divergent':59,'pending':52};(out/'census.json').write_text(json.dumps(dict(sha=sha,parent='3e4ab045924dd879fa2e3dfa0519d647808e99cd',before=dict(before),after=dict(counts),moved_rows=8,moved_symbols=moved,stopped_after_files=7),indent=2)+'\n');print('census',dict(counts),len(moved),flush=True)
with tempfile.TemporaryDirectory(prefix='blofin-order-reads-parity-',dir=sdk/'target') as temp:
 scope=pathlib.Path(temp);(scope/'Cargo.toml').write_text(re.sub(r'members = \[.*?\]','members = ["safe-cex-venue-blofin"]',(sdk/'Cargo.toml').read_text(),count=1,flags=re.S))
 for path in sdk.iterdir():
  if path.is_dir() and (path/'Cargo.toml').exists():(scope/path.name).symlink_to(path,target_is_directory=True)
 (scope/'exempt.toml').write_text('');cmd=[str(sdk/'target/debug/parity-check'),'--workspace',str(scope),'--manifest',str(sdk/'parity/manifest/exchanges/blofin'),'--exempt',str(scope/'exempt.toml')];p=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/'rust-parity.log').write_text(p.stdout);print('rust-parity',p.returncode,p.stdout,flush=True);assert p.returncode==1 and p.stdout.count('invention:')==3 and all('position_adapter.rs::' in line for line in p.stdout.splitlines() if line);(out/'rust-parity.json').write_text(json.dumps(dict(sha=sha,exit=p.returncode,command=cmd,binary_built_this_turn=False,scope='Blofin only; production symlinks; empty exemptions',expected_inherited_findings=3),indent=2)+'\n')
