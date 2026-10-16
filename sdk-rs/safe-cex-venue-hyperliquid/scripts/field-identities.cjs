const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const bases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/fixtures/types.json'),'utf8')).cases;
const cases=[],seen=new Set();
function visit(v,steps=[],out=[]) {
 if(typeof v==='number'||typeof v==='boolean'||(typeof v==='string'&&v.startsWith('wire-')))out.push(steps);
 else if(v&&typeof v==='object')for(const [k,x] of Object.entries(v))visit(x,[...steps,k],out);
 return out;
}
for(const base of bases) {
 if(base.name==='WsUserEvent')continue;
 const initial=JSON.parse(base.raw);let index=0;
 for(const steps of visit(initial)) {
  const changed=JSON.parse(base.raw);let parent=changed;for(const k of steps.slice(0,-1))parent=parent[k];
  const old=steps.length?parent[steps.at(-1)]:changed;
  const next=typeof old==='number'?101+(index++):typeof old==='boolean'?!old:'identity:'+steps.join('.')+':雪';
  let value=changed;if(steps.length)parent[steps.at(-1)]=next;else value=next;
  const raw=JSON.stringify(value),key=base.name+'\n'+raw;if(seen.has(key))continue;seen.add(key);cases.push({name:base.name,path:'/'+steps.join('/'),raw});
 }
}
if(process.argv.includes('--enums')) {
 cases.length=0;seen.clear();
 function strings(v,steps=[],out=[]) {if(typeof v==='string'&&!v.startsWith('wire-'))out.push(steps);else if(v&&typeof v==='object')for(const [k,x]of Object.entries(v))strings(x,[...steps,k],out);return out;}
 function read(v,steps) {for(const k of steps){if(v==null)return undefined;v=v[k];}return v;}
 for(const base of bases) {
  if(base.name==='WsUserEvent')continue;
  const input=JSON.parse(base.raw);
  for(const steps of strings(input)) {
   const current=read(input,steps);
   const alternatives=new Set(bases.filter(b=>b.name===base.name).map(b=>read(JSON.parse(b.raw),steps)).filter(v=>typeof v==='string'&&v!==current));
   for(const alternate of alternatives){let value=JSON.parse(base.raw);if(steps.length){let parent=value;for(const k of steps.slice(0,-1))parent=parent[k];parent[steps.at(-1)]=alternate;}else value=alternate;
    const raw=JSON.stringify(value),key=base.name+'\n'+raw;if(seen.has(key))continue;seen.add(key);cases.push({name:base.name,path:'/'+steps.join('/'),raw});
   }
  }
 }
}
const root=path.resolve(__dirname,'../../..'),file=path.join(root,'sdk-rs/safe-cex-venue-hyperliquid/tests/field-identity-probes.ts');
const text='import type * as Source from "../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types";\n'+cases.map((c,i)=>`const case${i}: Source.${c.name} = ${c.raw};`).join('\n');
const host=ts.createCompilerHost({}),read=host.readFile.bind(host),exists=host.fileExists.bind(host);host.readFile=f=>f===file?text:read(f);host.fileExists=f=>f===file||exists(f);
const program=ts.createProgram([file],{strict:true,noEmit:true,skipLibCheck:true,types:[],target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},host);
const errors=ts.getPreEmitDiagnostics(program);assert(!errors.some(e=>e.code===2307),'source imports must resolve');const ast=program.getSourceFile(file);const declarations=ast.statements.filter(ts.isVariableStatement);
const accepted=cases.filter((_,i)=>!errors.some(e=>e.file?.fileName===file&&e.start>=declarations[i].pos&&e.start<declarations[i].end));
const output=JSON.stringify({scope:'per-field distinguishing input accepted by the actual TS compiler; supplements unchanged base fixtures',cases:accepted},null,2)+'\n';
const names=[...new Set(accepted.map(c=>c.name))];const bindings=names.map(n=>`"${n}"=>roundtrip::<safe_cex_venue_hyperliquid::types::${n}>(raw),`).join('\n');
const fixture=path.join(__dirname,process.argv.includes('--enums')?'../tests/fixtures/enum-field-identities.json':'../tests/fixtures/field-identities.json'),arms=path.join(__dirname,process.argv.includes('--enums')?'../tests/fixtures/enum-field-identity-bindings.rs':'../tests/fixtures/field-identity-bindings.rs');const code='fn '+(process.argv.includes('--enums')?'dispatch_enum':'dispatch')+'(name:&str,raw:&str)->serde_json::Value {match name {\n'+bindings+'\n_=>panic!("unknown identity root {name}")}}\n';
if(process.argv.includes('--capture')){fs.writeFileSync(fixture,output,{flag:'wx'});fs.writeFileSync(arms,code,{flag:'wx'});}else{assert.equal(output,fs.readFileSync(fixture,'utf8'));assert.equal(code,fs.readFileSync(arms,'utf8'));}
console.log(accepted.length+' compiler-accepted field identity cases across '+names.length+' roots');
