"""Plausible signing/ordering/array defects must fail the maintained TS oracle."""
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
root=crate.parents[1]
source=crate/'src/api.rs'
original=source.read_text()
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-mexc','--test','api','--jobs','2','--','--exact','mexc_api_live_typescript_signatures_and_bodies','--nocapture']
controls=[('signMexcRequest','format!("{api_key}{request_time}{request_param}")','format!("{api_key}{request_time}{request_param}!")',1),
          ('serializeMexcParams','collator.compare(a, b)','a.cmp(b)',2),
          ('serializeMexcParams','prefix.to_owned()','format!("{prefix}[{key}]")',1),
          ('serializeMexcParams','if values.insert(key.as_str(), value).is_none() {','if { values.insert(key.as_str(), value); true } {',1)]
try:
    for name,before,after,count in controls:
        assert original.count(before)==count
        source.write_text(original.replace(before,after))
        result=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        assert result.returncode==101,result.stdout
        assert 'mexc_api_live_typescript_signatures_and_bodies ... FAILED' in result.stdout,result.stdout
        assert 'assertion `left == right` failed' in result.stdout and name in result.stdout,result.stdout
        print(f'Rust {name} control ({before}): named assertion fails, exit 101',flush=True)
finally:source.write_text(original)
subprocess.run(command,cwd=root,check=True)
