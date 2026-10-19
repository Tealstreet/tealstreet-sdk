'use strict';
const units = value => value.split('').map(character => character.charCodeAt(0));
let reads = 0;
const input = tag => ({
  undefined, null: null, number: 7, boolean: true, text: 'x',
  surrogate: String.fromCharCode(0xd800), object: {}, array: [], bigint: 3n,
  symbol: Symbol.iterator, map: new Map(), set: new Set(), promise: Promise.resolve(),
  regexp: /x/, nullproto: Object.create(null),
  poison: Object.defineProperty({}, 'toString', {get() { reads++; throw new Error('unreached'); }}),
})[tag];
const error = operation => {
  reads = 0;
  try { operation(); return {ok: true, reads}; }
  catch (failure) { return {ok: false, name: failure.name, units: units(failure.message), reads}; }
};
const tags = ['undefined', 'null', 'number', 'boolean', 'text', 'surrogate', 'object', 'poison', 'array', 'bigint', 'symbol'];
const callbacks = tags.flatMap(tag => ['array', 'map', 'set'].map(kind => ({
  tag, kind, ...error(() => {
    const value = input(tag);
    if (kind === 'array') [1].map(value);
    else if (kind === 'map') new Map([[1, 2]]).forEach(value);
    else new Set([1]).forEach(value);
  }),
})));
const from = tags.filter(tag => tag !== 'undefined').map(tag => ({
  tag, ...error(() => Array.from([1], input(tag))),
}));
const entries = ['text', 'surrogate', 'number', 'null', 'symbol'].map(tag => ({
  tag, ...error(() => new Map([input(tag)])),
}));
const executors = tags.filter(tag => !['object', 'poison', 'array'].includes(tag)).map(tag => ({
  tag, ...error(() => new Promise(input(tag))),
}));
const executorObjects = ['object', 'poison', 'array', 'map', 'set', 'promise', 'regexp', 'nullproto'].map(tag => ({
  tag, ...error(() => new Promise(input(tag))),
}));


const view = (value, token) => {
  if (value === token) return {reference: true};
  if (value === undefined) return {undefined: true};
  if (typeof value === 'number') {
    const bits = Buffer.alloc(8); bits.writeDoubleBE(value);
    return {bits: bits.readBigUInt64BE().toString()};
  }
  if (typeof value === 'string') return {units: units(value)};
  if (Array.isArray(value)) return Array.from({length:value.length}, (_,i) => i in value ? view(value[i],token) : {hole:true});
  return value;
};
const owners = {};
const record = (name, operation) => owners[name] = [0,1].map(profile => {
  const token = {};
  try { return {input:profile, output:{ok:true, value:view(operation(profile,token),token)}}; }
  catch (failure) { return {input:profile,output:{ok:false, same:failure===token,name:failure.name,units:units(failure.message||'')}}; }
});
const raw = token => [,undefined,token,-0,String.fromCharCode(0xd800)];
record('array_length',(p,t)=>p ? [].length : raw(t).length);
record('indexed',(p,t)=>{const a=raw(t),i=p?2:0;return i in a ? [true,a[i]] : [false];});
record('items',(p,t)=>(p?[undefined,t]:raw(t)).slice());
for(const n of ['flatten_into','flatten_length']) record(n,(p,t)=>[[,undefined,t],[-0,String.fromCharCode(0xd800)]].flat(p));
record('map_entries',(p,t)=>{const m=new Map();m.set(t,p?undefined:-0);return [m.get(t)===(p?undefined:-0),m.get(t)];});
record('member',(p,t)=>p?raw(t)['\ud800map']:raw(t).map(v=>v));
record('is_regexp',(p)=>p?({}) instanceof RegExp:new RegExp('x') instanceof RegExp);
record('require_callback',(p)=>{[1].map(p?String.fromCharCode(0xd800):v=>v);return undefined;});
record('array_from',(p,t)=>Array.from(raw(t),p?String.fromCharCode(0xd800):v=>v));
record('iterate',(p,t)=>p?({})[Symbol.iterator]:[...raw(t)]);
record('consume_iterator',(p,t)=>{const values=[];for(const v of raw(t)){if(p&&values.length===2)throw t;values.push(v);}return values;});
record('map_from_entries',(p,t)=>p?new Map([String.fromCharCode(0xd800)]):new Map([[t,String.fromCharCode(0xd800)]]).get(t));
record('not_iterable',(p)=>p?Array.from(null):new Set(7));
for(const n of ['json_stringify','stringify_json']) record(n,(p,t)=>JSON.stringify(p?Object.defineProperty({},'x',{enumerable:true,get(){throw t;}}):{x:String.fromCharCode(0xd800),absent:undefined,n:-0,inf:Infinity}));
record('v8_operand',(p)=>{try{[1].map(p?String.fromCharCode(0xd800):7);}catch(e){return e.message.slice(0,-' is not a function'.length);}});
record('install_builtins',(p,t)=>require('node:vm').runInNewContext('[sentinel===expected,typeof Array.from,typeof Map,JSON.stringify(raw)]',{sentinel:t,expected:t,raw:p?undefined:String.fromCharCode(0xd800)}));
const fs=require('node:fs'),path=require('node:path'),ts=require('typescript'),vm=require('node:vm');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/types.ts'),'utf8');
const ast=ts.createSourceFile('types.ts',source,ts.ScriptTarget.Latest,true);
const names=['OrderType','OrderStatus','OrderSide','OrderTimeInForce','OrderStopTriggerType','PositionSide','PositionMode','MarginType','LogSeverity'];
const enumSource=fs.readFileSync(path.resolve(__dirname,'../../../packages/logging/src/LogSeverity.ts'),'utf8')+'\n'+ast.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text)).map(n=>n.getText(ast)).join('\n');
const namespace={exports:{}};vm.runInNewContext(ts.transpileModule(enumSource,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,namespace);
record('install_enums',(p,t)=>{const g={sentinel:t};Object.assign(g,namespace.exports);if(p)Object.assign(g,namespace.exports);return [g.sentinel===t,...names.flatMap(n=>Object.entries(g[n]).map(([k,v])=>`${n}.${k}=${v}`))];});
record('construct_promise',(p)=>{new Promise(p?String.fromCharCode(0xd800):7);});
// Promise.finally keeps the original settlement referent after successful cleanup.
(async()=>{
  owners.chain_value=[];
  for(const p of [0,1]){const token={};try {const value=await(p?Promise.reject(token):Promise.resolve(token)).finally(()=>Promise.resolve());owners.chain_value.push({input:p,output:{ok:true,value:view(value,token)}});}catch(e){owners.chain_value.push({input:p,output:{ok:false,same:e===token,name:e.name,units:units(e.message||'')}});}}
  process.stdout.write(JSON.stringify({callbacks,from,entries,executors,executorObjects,owners}));
})();
