// Independently authored engine expectations for the five reviewed iterator
// owners. These execute JS operations; no Rust result is an expected value.
const output = { cache: [], object: [], next: [], collection: [], js: {} };
for (const [kind, method] of [['Map', 'values'], ['Set', 'entries']]) {
  const owner = kind === 'Map' ? new Map([['k', 13.5]]) : new Set([17.5]);
  const member = owner[method];
  output.cache.push({ kind, method, same: member === owner[method], value: member.call(owner).next().value });
}
for (const [kind, value] of [['Map', 23.5], ['Set', 29.5]]) {
  const owner = kind === 'Map' ? new Map([['k', value]]) : new Set([value]);
  const iterator = owner.values();
  output.object.push({ kind, value, tag: Object.prototype.toString.call(iterator), self: iterator[Symbol.iterator]() === iterator,
    ownNext: Object.hasOwn(iterator, 'next'), first: iterator.next() });
}
for (const [kind, value] of [['Map', 31.5], ['Set', 37.5]]) {
  const make = n => kind === 'Map' ? new Map([['k', n]]).values() : new Set([n]).values();
  const first = make(0), other = make(value), next = first.next;
  let error;
  try { next.call({}); } catch (failure) { error = failure.name; }
  output.next.push({ kind, value, step: next.call(other), error });
}
for (const values of [[3.5, 7.25], [11.5, 19.25, 23.5]]) {
  // Dense native access represents the ordered values retained by the JS Map.
  const map = new Map(values.map((value, index) => [index, value]));
  const view = [...map.values()];
  output.collection.push({ input: values, output: { len: view.length, values: view } });
}
output.js.get_property = ['plain', 'symbol'].map(key => {
  const owner = { plain: 'text', 'Symbol.iterator': 'string-key', [Symbol.iterator]() { return this; } };
  const value = owner[key === 'symbol' ? Symbol.iterator : key];
  return { input: key, output: typeof value === 'function' ? { kind: 'function', self: value.call(owner) === owner }
    : { kind: 'string', value } };
});
output.js.iterator = ['array', 'string'].map(kind => {
  const owner = kind === 'array' ? ['a', 'b'] : 'A😀';
  const iterator = owner[Symbol.iterator]();
  return { input: kind, output: { self: iterator[Symbol.iterator]() === iterator,
    values: [iterator.next().value, iterator.next().value], done: iterator.next().done } };
});
output.js.iterable_values = ['map', 'set'].map(kind => {
  const owner = kind === 'map' ? new Map([['a', 41.5], ['b', 43.5]]) : new Set([47.5, 53.5]);
  return { input: kind, output: [...owner] };
});
const bits = n => { const bytes = Buffer.alloc(8); bytes.writeDoubleBE(n); return bytes.toString('hex'); };
function view(value, token) {
  if (value === undefined) return ['undefined'];
  if (typeof value === 'number') return ['number', bits(value)];
  if (typeof value === 'string') return ['string', Array.from({length:value.length},(_,i)=>value.charCodeAt(i))];
  if (Array.isArray(value)) return value.map(v=>view(v,token));
  return ['reference',value===token];
}
output.nativeCollections = ['map-values','map-keys','map-entries','set-values','set-entries','map-clear','set-clear','map-reinsert','set-reinsert'].map(input => {
  const token={}, map=input.startsWith('map'), owner=map?new Map():new Set();
  const insert=(key,value)=>map?owner.set(key,value):owner.add(value);
  insert('a',-0); insert('b',token);
  const iterator=owner[input.endsWith('keys')?'keys':input.endsWith('entries')?'entries':'values']();
  const step=()=>{ const r=iterator.next(); return {done:r.done,value:view(r.value,token)}; };
  const steps=[step()];
  if(input.endsWith('clear')) owner.clear();
  else if(input.endsWith('reinsert')) owner.delete(map?'b':token);
  insert('c','\ud800');
  if(input.endsWith('reinsert')) insert('b',token);
  for(let i=0;i<4;i++)steps.push(step());
  insert('d',7); steps.push(step());
  return {input,output:{ok:true,self:iterator[Symbol.iterator]()===iterator,steps}};
});
output.nativeLookup=['own-getter','inherited-getter','own-throw','inherited-throw','own-data','inherited-data','own-absent','inherited-absent'].map(input=>{
  const token={}, prototype={}, owner={}, fn=function(){return this;}, receivers=[];
  const descriptor=input.endsWith('data')?{value:-0}:input.endsWith('absent')?{get:undefined}:{get(){receivers.push(this===owner);if(input.endsWith('throw'))throw token;return fn;}};
  Object.defineProperty(input.startsWith('inherited')?prototype:owner,Symbol.iterator,descriptor);
  if(input.startsWith('inherited'))Object.setPrototypeOf(owner,prototype);
  try {const value=owner[Symbol.iterator];return {input,output:{ok:true,value:value===fn?['method']:view(value,token),original:false,receivers}};}
  catch(error){return {input,output:{ok:false,value:null,original:error===token,receivers}};}
});
process.stdout.write(JSON.stringify(output));
