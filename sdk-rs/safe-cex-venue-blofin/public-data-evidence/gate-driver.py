import os,sys,json,subprocess,pathlib
out=pathlib.Path(sys.argv[1]);out.mkdir(exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');results=[]
commands=[('check',['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--all-targets','--jobs','2']),('host-clippy-crate-only',['cargo','clippy','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--all-targets','--jobs','2','--no-deps','--','-D','warnings']),('tests',['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','public_stream_data','--jobs','2','--','--nocapture']),('fmt',['rustfmt','--edition','2024','--check','sdk-rs/safe-cex-venue-blofin/src/public_stream_data.rs','sdk-rs/safe-cex-venue-blofin/tests/public_stream_data.rs'])]
for name,cmd in commands:
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha
 with(out/(name+'.log')).open('w')as f:r=subprocess.run(cmd,env=env,stdout=f,stderr=subprocess.STDOUT)
 results.append(dict(sha=sha,name=name,command=cmd,exit=r.returncode));(out/'cargo.json').write_text(json.dumps(dict(sha=sha,results=results),indent=2)+'\n');print(name,r.returncode,flush=True)
 if r.returncode:sys.exit(r.returncode)
 if name=='tests':assert '11 passed; 0 failed' in(out/'tests.log').read_text()
