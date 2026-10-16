import subprocess,pathlib,os,json,time,sys
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();base=sha[:10];ev=pathlib.Path('sdk-rs/safe-cex-venue-cryptocom/evidence/order-writes-'+base);prefix=sys.argv[1]if len(sys.argv)>1 else''
env=os.environ.copy();env['SAFE_CEX_TYPESCRIPT_PATH']='/Users/sam/cs/tealstreet-next/node_modules/typescript'
cargo='/opt/homebrew/opt/rustup/bin/cargo';common=['--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-cryptocom','--all-targets','--jobs','2','--locked'];yarn=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js'];records=[]
commands=[('venue-tests',[cargo,'test']+common+['--','--test-threads=1','--nocapture']),('check',[cargo,'check']+common),('host-clippy',[cargo,'clippy']+common+['--','-D','warnings']),('wasm-clippy',[cargo,'clippy']+common+['--target','wasm32-unknown-unknown','--','-D','warnings']),('fmt',[cargo,'fmt','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-cryptocom','--check']),('writer',yarn+['parity:extract']),('extractor',yarn+['parity:extract:check'])]
for name,command in commands:
 assert subprocess.run(['git','diff','--exit-code',sha,'--','sdk-rs/safe-cex-venue-cryptocom/src','sdk-rs/safe-cex-venue-cryptocom/tests'],stdout=subprocess.DEVNULL).returncode==0
 print('RUN '+prefix+name+' SHA '+sha,flush=True);start=time.time();log=ev/(prefix+name+'.log')
 with log.open('w')as f:result=subprocess.run(command,env=env,stdout=f,stderr=subprocess.STDOUT)
 record={'sha':sha,'name':name,'command':command,'env':{'SAFE_CEX_TYPESCRIPT_PATH':env['SAFE_CEX_TYPESCRIPT_PATH']},'exit':result.returncode,'seconds':round(time.time()-start,2),'log':log.name};content=log.read_text();record['result_text']=[line for line in content.splitlines()if any(key in line for key in ['test result:','Finished','error:','error[','safe-cex-transport/src/','Updated ','Would update ','added,','orphaned'])];records.append(record);(ev/(prefix+'gates.json')).write_text(json.dumps(records,indent=2)+'\n');print(prefix+name+' EXIT '+str(result.returncode),flush=True)
 if name in ['writer','extractor']:assert result.returncode==0 and ('Updated 0 manifests; 0 added, 0 orphaned symbols.'in content if name=='writer'else'Would update 0 manifests; 0 added, 0 orphaned symbols.'in content)
 elif name not in ['host-clippy','wasm-clippy']:assert result.returncode==0,(name,record)
