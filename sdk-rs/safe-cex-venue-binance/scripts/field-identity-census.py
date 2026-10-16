# Audit screen, not a parity gate. Capture: darwin-arm64, Node26.7.0.
# Counts equal sibling scalar observations; source classification remains manual.
import pathlib,subprocess,json,os,collections,itertools
root=pathlib.Path(__file__).resolve().parents[3]; env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH=str(root/'sdk-rs/target/binance-isolated-deps/node_modules/typescript'),PATH='/opt/homebrew/opt/rustup/bin:'+os.environ['PATH'])
report=[]
baseline=subprocess.check_output(['git','ls-tree','-r','--name-only','48a5682a73','--','sdk-rs/safe-cex-venue-binance/scripts'],cwd=root,text=True).splitlines()
for script in sorted(root/name for name in baseline if name.endswith('.mjs')):
 if script.name in ('signing-controls.mjs','signing.mjs','public-api.mjs'):continue
 command=['node',str(script)]+(['--oracle'] if script.name=='types.mjs' else [])
 result=subprocess.run(command,env=env,capture_output=True)
 if result.returncode: raise RuntimeError(f'{script.name}: {result.stderr.decode()}')
 try:data=json.loads(result.stdout)
 except ValueError:raise RuntimeError(f'Expected JSON corpus: {script.name}')
 pairs={};records=0
 def walk(x,path):
  global records
  if isinstance(x,dict):
   records+=1
   def scalar(v):
    if isinstance(v,dict) and len(v)==1:
     for tag in ('number','bits','numberBits'):
      if tag in v:return ('numeric-tag',v[tag])
    return v
   fields=[(k,scalar(v)) for k,v in x.items() if type(scalar(v)) in (str,int,float,bool,tuple)]
   for (a,av),(b,bv) in itertools.combinations(fields,2):
    typ=lambda v:'number' if type(v) in (int,float) else type(v).__name__
    if typ(av)!=typ(bv):continue
    key=(path,a,b,typ(av));row=pairs.setdefault(key,{'equal':0,'different':0,'sample':av})
    row['equal' if av==bv else 'different']+=1
   for k,v in x.items():
    walk(v,path+'.'+k)
    if k in ('raw','rows') and isinstance(v,str):
     try:parsed=json.loads(v)
     except ValueError:continue
     walk(parsed,path+'.'+k+'<JSON>')
  elif isinstance(x,list):
   for v in x:walk(v,path+'[]')
 walk(data,'$')
 collisions=[dict(path=k[0],a=k[1],b=k[2],type=k[3],**v) for k,v in pairs.items() if v['equal']]
 report.append(dict(script=script.name,records=records,pairs=len(pairs),collisions=collisions))
 print(script.name,'records',records,'pairs',len(pairs),'equalgroups',len(collisions),'alwaysEqual',sum(not x['different'] for x in collisions),flush=True)
(root/'sdk-rs/target/binance-field-census.json').write_text(json.dumps(report,indent=2))
