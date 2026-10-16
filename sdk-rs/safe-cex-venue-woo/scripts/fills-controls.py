"""Sequential field-name, unit and public-deserializer controls."""
from pathlib import Path
import subprocess,os
p=Path('sdk-rs/safe-cex-venue-woo/src/legacy_fills.rs');source=p.read_text()
cases=[
 ('size-name','pub size: f64,','#[serde(rename="limit")] pub size: f64,'),
 ('start-name','pub start_t: Option<f64>,','#[serde(rename="startTime")] pub start_t: Option<f64>,'),
 ('end-name','pub end_t: Option<f64>,','#[serde(rename="endTime")] pub end_t: Option<f64>,'),
 ('timestamp-unit','start_t: query.from_milliseconds,','start_t: query.from_milliseconds.map(|v|v/1000.0),'),
 ('zero-page','page: query.page,','page: query.page.filter(|v|*v!=0.0),'),
 ('symbol-falsy','query.symbol.clone().filter(|s| !s.is_empty())','query.symbol.clone()'),
 ('optional-null','T::deserialize(d).map(Some)','Option::<T>::deserialize(d)'),
 ('map-gate','let map = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;\n        QueryWire::deserialize(serde_json::Value::Object(map)).map_err(serde::de::Error::custom)','QueryWire::deserialize(d)'),
 ('public-inherent','// ts: buildWooLegacyFillsParams',"impl WooLegacyFillsQuery { pub fn deserialize<'de,D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error>{QueryWire::deserialize(d)} }\n// ts: buildWooLegacyFillsParams"),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,old,new in cases:
 assert old in source,(name,old)
 try:
  p.write_text(source.replace(old,new,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','legacy_fills','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-fills-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(source)
