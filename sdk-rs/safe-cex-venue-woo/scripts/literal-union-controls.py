"""Run sequentially from repo root; compilation failures never count as controls."""
from pathlib import Path
import os, subprocess, json, re
path=Path('sdk-rs/safe-cex-venue-woo/src/v3_contracts.rs')
original=path.read_text()
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
marker='enum WooV3HttpMethodWire {'
head,tail=original.split(marker)
needle='#[serde(rename = "GET")]'
assert tail.count(needle)==1
wide=original.replace('let value = String::deserialize(deserializer)?;', 'let value = serde_json::Value::deserialize(deserializer)?;')
wide=re.sub(r'::deserialize\(\s*serde::de::value::StringDeserializer::<\s*D::Error,?\s*>::new\(\s*value,?\s*\),?\s*\)', '::deserialize(value).map_err(serde::de::Error::custom)', wide)
cases=[('tagged-object',wide)]
for name,replacement in [('alias','#[serde(rename = "GET", alias = "HTTP_GET")]'),('rename','#[serde(rename = "GET_CHANGED")]'),('skip','#[serde(rename = "GET", skip)]')]:
    cases.append((name,head+marker+tail.replace(needle,replacement,1)))
cases.append(('public-inherent', original + "\nimpl WooV3HttpMethod { pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> { WooV3HttpMethodWire::deserialize(d) } }\n"))
results=[]
for name,mutated in cases:
    assert mutated!=original
    path.write_text(mutated)
    try:
        target=['--test','literal_calls'] if name=='public-inherent' else ['--lib','literal_union_actual']
        run=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo']+target,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        Path('/tmp/woo-union-control-'+name+'.log').write_text(run.stdout)
        ok=run.returncode==101 and 'test result: FAILED' in run.stdout
        results.append(dict(name=name,exit=run.returncode,assertion_failure=ok))
        print(name,run.returncode,'assertion-red' if ok else 'BAD',flush=True)
        assert ok,run.stdout
    finally:path.write_text(original)
Path('/tmp/woo-union-controls.json').write_text(json.dumps(results,indent=2))
