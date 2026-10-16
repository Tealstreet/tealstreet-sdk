from pathlib import Path
import os, subprocess
p=Path('sdk-rs/safe-cex-venue-grvt/src/builder.rs');original=p.read_text()
env=dict(os.environ,PATH='/opt/homebrew/bin:/opt/homebrew/opt/rustup/bin:'+os.environ['PATH'],SAFE_CEX_TYPESCRIPT_PATH=str(Path('sdk-rs/target/lodash-deps/node_modules/typescript').resolve()))
cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-venue-grvt','--test','builder_proxy','--jobs','1','builder_proxy_matches_actual_source','--','--exact','--nocapture']
for name,old,new,assertion in [('empty','Some(proxy) => format!("{}/{base_url}", proxy.url()),','Some(proxy) if !proxy.url().is_empty() => format!("{}/{base_url}", proxy.url()),\n        Some(_) => base_url.to_owned(),','BUILDER-PROXY native url'),('network','if testnet { 5 } else { 2 }','if testnet { 2 } else { 5 }','BUILDER-PROXY native edge')]:
    assert original.count(old)==1
    try:
        p.write_text(original.replace(old,new))
        r=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        Path('sdk-rs/target/builder-proxy-'+name+'.log').write_text(r.stdout)
        assert r.returncode==101 and assertion in r.stdout and 'Finished `test`' in r.stdout,(name,r.returncode,r.stdout)
        print(name,101,assertion,flush=True)
    finally: p.write_text(original)
r=subprocess.run(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
Path('sdk-rs/target/builder-proxy-restored.log').write_text(r.stdout)
assert r.returncode==0,r.stdout
print('restored pass')
