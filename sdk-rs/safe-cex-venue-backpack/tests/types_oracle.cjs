const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const {schema,sourcePath,ts}=require('./type_schema.cjs');
const source=fs.readFileSync(sourcePath,'utf8');
const live=schema(source);
assert.deepEqual(live,JSON.parse(fs.readFileSync(path.join(__dirname,'type_schema.json'),'utf8')),'complete source field census');
assert.deepEqual(schema('interface Parent { inherited: number; } export interface Child extends Parent { own: string; }').Child.map(f=>f.name),['inherited','own'],'inherited required field control');
assert.throws(()=>schema('export interface Child extends Missing { own: string; }'), /resolved heritage/, 'unresolved heritage fails closed');
const cases=[];let sequence=0;
function sample(t,p,mode='full') {
  switch(t.kind) {
    case 'string':return `${p}:λ🚀`;
    case 'number':return ++sequence+0.125;
    case 'boolean':return (++sequence%2)===0;
    case 'literal':return t.value;
    case 'array':return [sample(t.item,p+'[0]',mode),sample(t.item,p+'[1]',mode)];
    case 'tuple':return t.items.map((v,i)=>sample(v,p+'['+i+']',mode));
    case 'union':return mode==='null'&&t.items.some(v=>v.kind==='null')?null:sample(t.items.find(v=>v.kind!=='null'),p,mode);
    case 'ref':return t.name==='T'?++sequence+0.125:record(t.name,live[t.name],p,mode);
    case 'object':return record('',t.fields,p,mode);
    default:throw new Error('unsupported sample '+t.kind);
  }
}
function record(name,fields,p,mode) {return Object.fromEntries(fields.filter(f=>mode!=='minimal'||!f.optional).map(f=>[f.name,sample(f.type,p+'.'+f.name,mode)]));}
function capture(value) {
  if(typeof value==='number') {
    if(Number.isNaN(value))return '7ff8000000000000';
    const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex');
  }
  if(Array.isArray(value))return value.map(capture);
  if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,capture(v)]));
  return value;
}
// Each wire token is parsed by actual JavaScript and captured BEFORE JSON output.
function add(name,label,raw,accept=true,utf16=false) {
 const value=JSON.parse(raw);const lone=[];
 function check(v) {
  if(typeof v==='string') {
   for(const c of v)if(c.length===1&&c.charCodeAt(0)>=0xd800&&c.charCodeAt(0)<=0xdfff) {
    assert.equal(typeof v,'string','TypeScript lone surrogate typeof before serialization');
    lone.push(c.charCodeAt(0));
   }
  } else if(v&&typeof v==='object')Object.values(v).forEach(check);
 }
 if(utf16) {check(value);assert.equal(lone.length,1,'TypeScript lone surrogate retained before serialization');}
 cases.push({name,label,raw,accept,native:accept?capture(value):null,wire:accept?JSON.stringify(value):null,lone});
}
let typed='';let id=0;
function expr(value){return JSON.stringify(value);}
for(const [name,fields] of Object.entries(live)) {
 const type=name==='BackpackApiResponse'?`${name}<number>`:name;
 for(const mode of ['full','minimal','null']) {
  const value=record(name,fields,name,mode);
  typed+=`const valid${id++}: ${type} = ${expr(value)};\n`;
  add(name,mode,expr(value));
 }
 const value=record(name,fields,name,'full');
 function reject(changed,label) {
  typed+=`// @ts-expect-error ${name} ${label}\nconst invalid${id++}: ${type} = ${expr(changed)};\n`;
  add(name,label,expr(changed),false);
 }
 function mutate(p,replacement,remove=false) {
  const changed=structuredClone(value);let parent=changed;
  for(const part of p.slice(0,-1))parent=parent[part];
  if(remove)delete parent[p.at(-1)];else parent[p.at(-1)]=replacement;
  return changed;
 }
 function checkFields(fields,v,p=[]) {
  for(const f of fields) {
   const fp=[...p,f.name];const t=f.type;
   if(!(t.kind==='union'&&t.items.some(t=>t.kind==='null')))reject(mutate(fp,null),'null '+fp.join('.'));
   if(!f.optional)reject(mutate(fp,undefined,true),'missing '+fp.join('.'));
   nested(t,v[f.name],fp);
  }
 }
 function nested(t,v,p) {
  if(t.kind==='ref'&&t.name!=='T')checkFields(live[t.name],v,p);
  else if(t.kind==='object') {
   reject(mutate(p,Object.values(v)),'positional '+p.join('.'));
   checkFields(t.fields,v,p);
  } else if(t.kind==='array')v.forEach((x,i)=>nested(t.item,x,[...p,i]));
  else if(t.kind==='tuple')t.items.forEach((x,i)=>nested(x,v[i],[...p,i]));
  else if(t.kind==='union')t.items.filter(x=>x.kind!=='null').forEach(x=>nested(x,v,p));
  else if(t.kind==='literal') {
   for(const invalid of ['__unknown__', t.value.toUpperCase()===t.value?t.value.toLowerCase():t.value.toUpperCase(), {[t.value]:null}, [t.value]]) {
    const allowed=t.kind==='literal'&&invalid===t.value;
    if(!allowed)reject(mutate(p,invalid),'literal refusal '+p.join('.'));
   }
  }
 }
 checkFields(fields,value);
 // Boolean field swaps need independent one-hot patterns: two Boolean values
 // alone cannot give every same-typed property a unique sentinel.
 for(const f of fields.filter(f=>f.type.kind==='boolean')) {
  const oneHot={...value};for(const b of fields.filter(f=>f.type.kind==='boolean'))oneHot[b.name]=b.name===f.name;
  add(name,'boolean '+f.name,expr(oneHot));
 }
 function leaves(t,v,p) {
  if(t.kind==='number'||(t.kind==='ref'&&t.name==='T'))return [{kind:'number',p}];
  if(t.kind==='string')return [{kind:'string',p}];
  if(t.kind==='literal')return [{kind:'literal',p,values:[t.value]}];
  if(t.kind==='union') {
   const nonnull=t.items.filter(x=>x.kind!=='null');
   if(nonnull.every(x=>x.kind==='literal'))return [{kind:'literal',p,values:nonnull.map(x=>x.value)}];
   return leaves(nonnull[0],v,p);
  }
  if(t.kind==='ref')return live[t.name].flatMap(f=>leaves(f.type,v[f.name],[...p,f.name]));
  if(t.kind==='object')return t.fields.flatMap(f=>leaves(f.type,v[f.name],[...p,f.name]));
  if(t.kind==='array')return v.flatMap((x,i)=>leaves(t.item,x,[...p,i]));
  if(t.kind==='tuple')return t.items.flatMap((x,i)=>leaves(x,v[i],[...p,i]));
  return [];
 }
 for(const leaf of fields.flatMap(f=>leaves(f.type,value[f.name],[f.name]))) {
  const tokens=leaf.kind==='number'?['-0','9007199254740993','1e309','-1e-999']:leaf.kind==='string'?['"\\ud800"','"\\udc00"']:leaf.values.map(v=>JSON.stringify(v));
  for(const token of tokens) {
   const changed=structuredClone(value);let parent=changed;for(const part of leaf.p.slice(0,-1))parent=parent[part];
   parent[leaf.p.at(-1)]='__RAW_TOKEN__';
   const raw=JSON.stringify(changed).replace('"__RAW_TOKEN__"',token);
   add(name,`${leaf.p.join('.')} ${token}`,raw,leaf.kind!=='string',leaf.kind==='string');
  }
 }
 add(name,'positional array',JSON.stringify(Object.values(value)),false);
}
for(const token of ['-0','9007199254740993','1e309','-1e-999']) {
 const raw=`{"data":{"serverTime":${token}}}`;
 typed+=`const generic${id++}: BackpackApiResponse<BackpackTime> = ${raw};\n`;
 add('BackpackApiResponseRetained','retained generic '+token,raw);
}
// Compile the witnesses against the ACTUAL declarations, including every
// required-field refusal. Erased interfaces have no runtime body to reimplement.
const harness=source+'\n'+typed;
const options={strict:true,skipLibCheck:true,noEmit:true,types:[],target:ts.ScriptTarget.ES2022};
const host=ts.createCompilerHost(options);const original=host.getSourceFile.bind(host);
host.getSourceFile=(file,language,onError,shouldCreate)=>path.resolve(file)===sourcePath?ts.createSourceFile(file,harness,language,true):original(file,language,onError,shouldCreate);
const program=ts.createProgram([sourcePath],options,host);
const diagnostics=ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length,0,ts.formatDiagnosticsWithColorAndContext(diagnostics,{getCurrentDirectory:()=>process.cwd(),getCanonicalFileName:n=>n,getNewLine:()=> '\n'}));
vm.runInNewContext(ts.transpileModule(harness,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:{}},{filename:sourcePath});
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases,interfaces:Object.keys(live).length}));
