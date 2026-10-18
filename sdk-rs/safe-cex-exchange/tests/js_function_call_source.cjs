'use strict';
const assert = require('node:assert/strict');
const bits = value => { const bytes=Buffer.alloc(8); bytes.writeDoubleBE(value); return bytes.toString('hex'); };
const describe = value => {
  if (value === null) return {kind:'null'};
  if (typeof value === 'number') return {kind:'number',bits:bits(value)};
  if (typeof value === 'string') return {kind:'string',units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};
  if (typeof value === 'bigint') return {kind:'bigint',text:String(value)};
  if (typeof value === 'boolean') return {kind:'boolean',value};
  return {kind:typeof value};
};
const receivers=[undefined,null,false,7,-0,NaN,'\ud800',{},function receiver(){},123n];
const rows=[];
for (const receiver of receivers) {
  const object={marker:1},args=[object,-0,'\ud800',undefined,123n];
  let observed;
  const target=function(...values){observed={thisValue:this,values};return values[0];};
  const result=target.call(receiver,...args);
  assert(Object.is(observed.thisValue,receiver));
  assert.equal(observed.values.length,5);
  observed.values.forEach((value,i)=>assert(Object.is(value,args[i])));
  assert.equal(result,object);
  rows.push({receiver:describe(observed.thisValue),arguments:observed.values.map(describe),resultSame:true});
}
let empty;
(function(){empty={receiver:describe(this),arity:arguments.length};}).call();
assert.deepEqual(empty,{receiver:{kind:'undefined'},arity:0});
const call=Function.prototype.call;
const refusals=[];
for(const invalid of [undefined,null,false,7,'x',{},[]]) {
  let thrown;
  try { Reflect.apply(call,invalid,[]); } catch(error) { thrown=error; }
  assert(thrown instanceof TypeError);
  refusals.push({receiver:describe(invalid),error:thrown.name});
}
let refusalReads=0,refusalCalls=0,refusal;
const invalid={get toString(){refusalReads++;return function(){refusalCalls++;throw new Error('conversion must not run');};}};
try { Reflect.apply(call,invalid,[]); } catch(error) { refusal=error; }
assert(refusal instanceof TypeError);assert.equal(refusalReads,0);assert.equal(refusalCalls,0);
const refusalHooks={reads:refusalReads,calls:refusalCalls,error:refusal.name};
let reads=0,calls=0;
const target=function(){throw new Error('original body must not run');};
const receiver={};
const sentinel={};
Object.defineProperty(target,'call',{get(){reads++;return function(value){calls++;assert.equal(this,target);assert.equal(value,receiver);return sentinel;};}});
assert.equal(target.call(receiver),sentinel);
assert.equal(reads,1);assert.equal(calls,1);
for(const marker of [{},new TypeError('original')]) {
  let caught;
  try {(function(){throw marker;}).call(receiver);}catch(error){caught=error;}
  assert.equal(caught,marker);
}
const promise=Promise.resolve(7);
assert.equal((()=>promise).call(receiver),promise);
assert.equal(rows.length,10);assert.equal(refusals.length,7);
process.stdout.write(JSON.stringify({rows,empty,refusals,refusalHooks,ownCall:{reads,calls},throws:2,promiseSame:true}));
