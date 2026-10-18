const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const numeric = JSON.parse(fs.readFileSync(path.join(__dirname, '../tests/fixtures/typed-numbers.json'), 'utf8'));
const bases = JSON.parse(fs.readFileSync(path.join(__dirname, '../tests/fixtures/types.json'), 'utf8')).cases;
const escape = key => String(key).replaceAll('~','~0').replaceAll('/','~1');
function paths(value, steps = []) {
  if (typeof value === 'string' && value.startsWith('wire-')) return [steps];
  if (value && typeof value === 'object') return Object.entries(value).flatMap(([key,child]) => paths(child,[...steps,key]));
  return [];
}
function strings(value, prefix = '', result = {}) {
  if (typeof value === 'string') result[prefix] = Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
  else if (value && typeof value === 'object') for (const [key,child] of Object.entries(value)) strings(child,`${prefix}/${escape(key)}`,result);
  return result;
}
const cases = [], names = [];
for (const name of numeric.names) {
  const base = bases.find(base => base.name === name && paths(JSON.parse(base.raw)).length);
  if (!base) continue;
  names.push(name); const steps = paths(JSON.parse(base.raw))[0];
  for (const input of ['\ud800','\udfff','雪😀\ufeff']) {
    let value = JSON.parse(base.raw);
    if (steps.length) {let parent=value;for(const key of steps.slice(0,-1)) parent=parent[key];parent[steps.at(-1)]=input;}else value=input;
    const raw = JSON.stringify(value), parsed = JSON.parse(raw);
    cases.push({name,path:'/'+steps.map(escape).join('/'),raw,refusal:input==='\ud800'||input==='\udfff',sourceJson:JSON.stringify(parsed),strings:strings(parsed)});
  }
}
const text=JSON.stringify({names,cases},null,2)+'\n';
const fixture=path.join(__dirname,'../tests/fixtures/typed-string-boundaries.json');
const bindings=path.join(__dirname,'../tests/fixtures/typed-string-bindings.rs');
const arms='// Source-derived string boundary inventory.\nfn capture_typed_strings(name:&str,raw:&str)->Result<std::collections::BTreeMap<String,Vec<u16>>,serde_json::Error>{\n match name {\n'+names.map(name=>` "${name}"=>capture_strings::<safe_cex_venue_hyperliquid::types::${name}>(raw),`).join('\n')+'\n _=>panic!("unrecognized source string-bearing numeric type {name}"),\n }\n}\n';
if(process.argv.includes('--capture')){assert.ok(!fs.existsSync(fixture)&&!fs.existsSync(bindings),'exclusive new string paths');fs.writeFileSync(fixture,text,{flag:'wx'});fs.writeFileSync(bindings,arms,{flag:'wx'});}
else{assert.equal(text,fs.readFileSync(fixture,'utf8'),'live typed-string boundary drift');assert.equal(arms,fs.readFileSync(bindings,'utf8'),'typed-string binding inventory drift');}
console.log(`${names.length} shared string-boundary citations; ${cases.length} source witnesses`);
