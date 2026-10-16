"""Substitute every map gate independently; require runtime assertion failures."""
from pathlib import Path
import os, re, subprocess, sys
root=Path('sdk-rs/safe-cex-venue-woo')
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
seen=set()
for module in ['types','v3_contracts','v3_public','legacy_fills']:
 path=root/f'src/{module}.rs';original=path.read_text()
 pattern=r'(\w+)Wire::deserialize\(\s*safe_cex_types::strict_decode::object\(\s*decoder,?\s*\),?\s*\)'
 matches=list(re.finditer(pattern,original))
 assert len(matches)=={'types':15,'v3_contracts':12,'v3_public':6,'legacy_fills':1}[module], f'{module}: map-gate inventory changed ({len(matches)})'
 for m in matches:
  name=m[1]
  seen.add(name)
  if len(sys.argv)>1 and name not in sys.argv[1:]:continue
  mutations=[('map-gate',original[:m.start()]+f'{name}Wire::deserialize(decoder)'+original[m.end():])]
  # Flattened Serde companions already reject sequences without our map gate.
  # Exercise an actual widening instead: a public positional adapter must not
  # bypass the trait path merely because the private flatten decoder is strict.
  if name=='WooV3MappedOrderBookSnapshot':
   mutations=[('public-positional',original+f"\nimpl {name} {{ pub fn deserialize<'de, D: serde::Deserializer<'de>>(d:D)->Result<Self,D::Error> {{ let value=serde_json::Value::deserialize(d)?; let value=if let serde_json::Value::Array(values)=value {{ serde_json::Value::Object([\"bids\",\"asks\",\"timestamp\"].into_iter().map(String::from).zip(values).collect()) }} else {{ value }}; <Self as serde::Deserialize>::deserialize(value).map_err(serde::de::Error::custom) }} }}\n")]
  # The public qualified call must not be shadowable by a permissive inherent method.
  if name=='WooV3PageMeta':
   mutations.append(('public-inherent',original+f"\nimpl {name} {{ pub fn deserialize<'de, D: serde::Deserializer<'de>>(d:D)->Result<Self,D::Error> {{ {name}Wire::deserialize(d) }} }}\n"))
  for kind,mutated in mutations:
   try:
    path.write_text(mutated)
    run=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','--offline','--jobs','2','-p','safe-cex-venue-woo','--test',{'v3_public':'v3_public','legacy_fills':'legacy_fills'}.get(module,'record_calls')],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    Path(f'/tmp/woo-record-{name}-{kind}.log').write_text(run.stdout)
    ok=run.returncode==101 and 'test result: FAILED' in run.stdout and 'positional arrays accepted' in run.stdout
    print(name,kind,run.returncode,'assertion-red' if ok else 'BAD',flush=True)
    assert ok,run.stdout
   finally:path.write_text(original)

assert set(sys.argv[1:]) <= seen, 'unknown map control name'
