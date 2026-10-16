const assert = require('node:assert/strict');
const {schema} = require('./type-schema.cjs');
const {load,root} = require('../tests/configuration.cjs');
const path = require('node:path');
// Executes the full actual module, including all runtime constant declarations.
load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'));
function observe(value) {
  if(typeof value==='number') {
    const bytes=Buffer.alloc(8); bytes.writeDoubleBE(Number.isNaN(value)?NaN:value);
    return {number: Number.isNaN(value)?'7ff8000000000000':bytes.toString('hex')};
  }
  if(Array.isArray(value)) return value.map(observe);
  if(value!==null && typeof value==='object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
  return value;
}
let serial=0; let booleans=0;
function fixture(type,mode,location) {
  const id=++serial;
  switch(type.kind) {
    case 'ref': return fixture(schema.declarations[type.name],mode,location);
    case 'object': return '{'+type.fields.filter(f=>!(mode===7&&f.optional)).map(f=>JSON.stringify(f.name)+':'+fixture(f.type,mode,location+'.'+f.name)).join(',')+'}';
    case 'array': return '['+[0,1].map(i=>fixture(type.item,mode,location+'.'+i)).join(',')+']';
    case 'tuple': return '['+type.items.map((t,i)=>fixture(t,mode,location+'.'+i)).join(',')+']';
    case 'union': return fixture(type.arms[mode%type.arms.length],mode,location);
    case 'enum': return JSON.stringify(type.values[(mode+id)%type.values.length]);
    case 'number_string': return mode===6 ? JSON.stringify('union:'+location) : fixture({kind:'number'},mode,location);
    case 'number': return [String(id+0.125),'-0','1e400','-1e400','-1e-4000','9007199254740993',String(-id-0.75),String(id+0.5)][mode];
    case 'string': return JSON.stringify('str:'+location+':'+id+':😀');
    case 'boolean': return String((booleans++ + mode)%2===0);
    case 'any': return '{"n":'+fixture({kind:'number'},mode,location+'.n')+',"text":"opaque","nested":[null,true]}';
    default: throw Error('unsupported fixture '+type.kind);
  }
}
const cases=[];
for(const name of schema.exported) {
  for(let mode=0;mode<8;mode++) {
    serial=0; booleans=0;
    const input=fixture(schema.declarations[name],mode,name);
    const actual=JSON.parse(input);
    if(mode===0 && name==='WS_ORDER_TRADE_UPDATE') assert.notEqual(actual.o.R,actual.o.m,'WS boolean fixture distinguishes fields');
    if(mode===0 && name==='FETCH_ORDERS_RESPONSE') assert.notEqual(actual.data[0].triggerPriceType,actual.data[0].triggerType,'trigger enum fixture distinguishes fields');
    cases.push({name,label:'mode-'+mode,input,expected:observe(actual),accepted:true});
  }
  serial=0; booleans=0;
  const base=JSON.parse(fixture(schema.declarations[name],0,name));
  function visit(type,segments) {
    if(type.kind==='ref') return visit(schema.declarations[type.name],segments);
    if(type.kind==='array') return visit(type.item,[...segments,0]);
    if(type.kind==='union') return visit(type.arms[0],segments);
    if(type.kind!=='object') return;
    for(const field of type.fields) {
      for(const mutation of ['missing','null']) {
        const input=structuredClone(base); const parent=segments.reduce((v,k)=>v[k],input);
        if(mutation==='missing') delete parent[field.name]; else parent[field.name]=null;
        // `any` accepts null; other fields match the declared, non-null type.
        const accepted=mutation==='missing'?field.optional:field.type.kind==='any';
        cases.push({name,label:[...segments,field.name,mutation].join('.'),input:JSON.stringify(input),accepted,expected:accepted?observe(input):null});
      }
      visit(field.type,[...segments,field.name]);
    }
    const input=structuredClone(base);
    if(segments.length) {const parent=segments.slice(0,-1).reduce((v,k)=>v[k],input);parent[segments.at(-1)]=Object.values(parent[segments.at(-1)]);}
    cases.push({name,label:segments.join('.')+'.record-array',input:JSON.stringify(segments.length?input:Object.values(base)),accepted:false});
  }
  visit(schema.declarations[name],[]);
}
assert.equal(schema.exported.length,10,'exported type witness census');
// A valid JavaScript string can contain a lone UTF-16 surrogate. Rust's shared
// UTF-8 policy deliberately refuses it, separately from ordinary Unicode cases.
for (const name of schema.exported) {
  if (name === 'BITRUE_ORDER_TYPE' || name === 'ORDER_BOOK_WS_UPDATE') continue;
  serial=0; booleans=0;
  const original=fixture(schema.declarations[name],0,name);
  const input=original.replace(/"str:[^"]*"/, '"\\ud800"');
  assert.notEqual(input,original,'surrogate fixture '+name);
  const parsed=JSON.parse(input);
  function hasSurrogate(v) { return typeof v==='string'?v.charCodeAt(0)===0xd800:Array.isArray(v)?v.some(hasSurrogate):v&&typeof v==='object'?Object.values(v).some(hasSurrogate):false; }
  assert.ok(hasSurrogate(parsed),'live UTF-16 source value '+name);
  cases.push({name,label:'chosen UTF-8 surrogate refusal',input,accepted:false});
}
process.stderr.write(JSON.stringify({capture_header:require('./capture-header.cjs')()})+'\n');
process.stdout.write(JSON.stringify(cases));
