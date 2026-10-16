import pathlib,subprocess,os,json,time,re,difflib,hashlib
root=pathlib.Path.cwd();sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();assert subprocess.run(['git','diff','--quiet','HEAD','--']).returncode==0
out=pathlib.Path('sdk-rs/safe-cex-venue-bitrue/evidence/audit-remediation')/sha[:10];out.mkdir(parents=True,exist_ok=True)
env=os.environ.copy();env['RUSTFLAGS']='-Dwarnings'
records=[]
def run(name,argv):
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha
 print('RUN',name,sha,flush=True);start=time.time();r=subprocess.run(argv,env=env,text=True,capture_output=True);s=r.stdout+r.stderr;s=s.replace(str(root),'<worktree>');s=s.replace(env['SAFE_CEX_TYPESCRIPT_PATH'],'<external-typescript>') if env.get('SAFE_CEX_TYPESCRIPT_PATH') else s;s=re.sub(r'/Users/[^\s\)\"\']+','<external-path>',s).replace('/opt/homebrew','<toolchain>');p=out/(name+'.log');p.write_text(s)
 row={'sha':sha,'name':name,'command':argv,'exit':r.returncode,'seconds':round(time.time()-start,2),'environment':['RUSTFLAGS=-Dwarnings','SAFE_CEX_TYPESCRIPT_PATH supplied externally'],'log':p.name,'log_sha256':hashlib.sha256(s.encode()).hexdigest()};records.append(row);(out/'gates.json').write_text(json.dumps(records,indent=2)+'\n');print(name,'EXIT',r.returncode,flush=True);return row,s
common=['--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bitrue','--locked']
for name,cmd in [('fmt',['cargo','fmt','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bitrue','--check']),('check',['cargo','check',*common,'--all-targets','--jobs','2']),('tests',['cargo','test',*common,'--all-targets','--jobs','1','--','--test-threads=1']),('host-clippy',['cargo','clippy',*common,'--all-targets','--jobs','2']),('wasm-clippy',['cargo','clippy',*common,'--all-targets','--target','wasm32-unknown-unknown','--jobs','2'])]:
 row,text=run(name,cmd)
 if row['exit']!=0:raise SystemExit('STOP gate '+name)
# Restore each source exactly before another control. No source reviews during mutation.
def matching(s,start,opening,closing):
 depth=0;quoted=False;escape=False
 for i in range(start,len(s)):
  c=s[i]
  if quoted:
   if escape:escape=False
   elif c=='\\':escape=True
   elif c=='"':quoted=False
   continue
  if c=='"':quoted=True
  elif c==opening:depth+=1
  elif c==closing:
   depth-=1
   if depth==0:return i
 raise AssertionError('unbalanced')
def function(s,name,change):
 start=s.index('pub fn '+name+'<');brace=s.index('{',start);end=matching(s,brace,'{','}');body=s[brace+1:end];return s[:brace+1]+change(body)+s[end:]
def delay_get(body):
 a=body.index('get(');end=matching(body,a+3,'(',')');call=body[a:end+1];return body[:a]+'Ok::<_, ExchangeError>(async move { '+call+'?.await })'+body[end+1:]
def delay_line(body):
 a=body.index('let request =');end=body.index(';',a)+1;line=body[a:end];body=body[:a]+body[end:];needle='async move {';assert body.count(needle)>=1;return body.replace(needle,needle+'\n'+line,1)
def replace(old,new):
 def go(body):
  assert body.count(old)==1,(old,body.count(old));return body.replace(old,new)
 return go
variants=[('sync','leverage_reads.rs','fetch_and_sync_symbol_max_leverage',delay_get,'entry_timing','ENTRY-sync'),('leverage','leverage_reads.rs','fetch_symbol_max_leverage',delay_line,'entry_timing','ENTRY-leverage'),('update','leverage_reads.rs','update_symbol_metadata',delay_line,'entry_timing','ENTRY-update'),('raw','account_reads.rs','fetch_balance_and_positions_raw',delay_line,'entry_timing','ENTRY-raw'),('balance','account_reads.rs','fetch_balance',delay_line,'entry_timing','ENTRY-balance'),('position','position_metadata.rs','fetch_position_metadata',replace('Err(fetch_account())','Err(Ok::<_, ExchangeError>(async move { fetch_account()?.await }))'),'entry_timing','ENTRY-position'),('cache','position_metadata.rs','fetch_position_metadata',replace('Ok(finish_metadata(\n            cache,\n            symbols,\n            supplied.map(<[_]>::to_vec),\n            &is_disposed,\n        ))','Ok(Ok(Vec::new()))'),'entry_timing','ENTRY-CACHE-WRITE'),('sync-emit','account_reads.rs','fetch_balance_and_positions_raw',replace('map_err(|error| emit_account_error(error, emit.take().unwrap()))','map_err(|_error| Ok(None))'),'entry_timing','ENTRY-SYNC-ERROR-EMIT'),('native-key','read_helpers.rs','parse_max_leverage_from_bracket_response',replace('== Some(symbol)','== Some({ let _ = symbol; "A" })'),'boundary_prices','PRICE-NATIVE-SCALAR-KEY')]
controls=[]
for index,(name,file,func,change,test,assertion) in enumerate(variants):
 p=pathlib.Path('sdk-rs/safe-cex-venue-bitrue/src')/file;original=p.read_bytes();src=original.decode()
 # The existing synchronous parser has no generic parameters.
 if name=='native-key':
  a=src.index('pub fn '+func+'(');brace=src.index('{',a);end=matching(src,brace,'{','}');mutant=src[:brace+1]+change(src[brace+1:end])+src[end:]
 else:mutant=function(src,func,change)
 patch=''.join(difflib.unified_diff(src.splitlines(True),mutant.splitlines(True),fromfile=str(p),tofile=str(p)));(out/(name+'.patch')).write_text(patch)
 command=['cargo','test',*common,'--test',test,'--jobs','1','--','--test-threads=1','--nocapture']
 try:p.write_text(mutant);broken,log=run(name+'-mutated',command)
 finally:p.write_bytes(original)
 restored,restorelog=run(name+'-restored',command)
 valid=broken['exit']==101 and 'running 1 test' in log and assertion in log and 'could not compile' not in log
 controls.append({'sha':sha,'iteration':index,'name':name,'assertion':assertion,'mutated':broken['exit'],'restored':restored['exit'],'runtime_discrimination':valid,'patch':name+'.patch','exact_source_restored':p.read_bytes()==original})
 (out/'controls.json').write_text(json.dumps({'sha':sha,'expected_controls':9,'stop':f'index{index} {name}','controls':controls},indent=2)+'\n')
 if not valid or restored['exit']!=0:raise SystemExit('STOP control '+name+' runtime='+str(valid))
assert subprocess.run(['git','diff','--quiet','HEAD','--']).returncode==0
print('COMPLETE gates5x0, controls9x101->0; exact restored',flush=True)
