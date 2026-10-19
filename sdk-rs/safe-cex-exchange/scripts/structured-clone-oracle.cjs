const assert = require('node:assert/strict');
const bits = n => {
  if (Number.isNaN(n)) return 'NaN';
  const b = new DataView(new ArrayBuffer(8)); b.setFloat64(0, n);
  return b.getBigUint64(0).toString(16).padStart(16, '0');
};
const descriptor = (object, name) => {
  const d = Object.getOwnPropertyDescriptor(object, name);
  return [d.writable, d.enumerable, d.configurable];
};
const cases = [];
function profile(input, operation) { cases.push({input, output: operation()}); }
profile('primitives', () => {
  const c = structuredClone([undefined, null, true, -0, 0, NaN, Infinity, -Infinity, '\ud800X\udc00', 9007199254740993n]);
  return {ownUndefined: Object.hasOwn(c, 0) && c[0] === undefined, nil: c[1] === null, bool: c[2], numbers: c.slice(3,8).map(bits), units: Array.from({length:c[8].length},(_,i)=>c[8].charCodeAt(i)), big:String(c[9])};
});
profile('graph', () => {
  const child = {n:17}, root = {child, again:child}; root.self=root;
  root.map=new Map([[child,root]]); root.set=new Set([child,root]);
  const c=structuredClone(root);
  const result={detached:c!==root, self:c.self===c, alias:c.child===c.again, mapKey:[...c.map.keys()][0]===c.child, mapValue:c.map.get(c.child)===c, set:[c.set.has(c.child),c.set.has(c)], value:c.child.n};
  child.n=99; assert.equal(c.child.n,17); return result;
});
profile('sparse', () => {
  const a=[]; a.length=4; a[1]=undefined; a[3]=-0; a.extra='\ud800';
  const c=structuredClone(a);
  return {length:c.length, keys:Object.keys(c), hole:!Object.hasOwn(c,0), ownUndefined:Object.hasOwn(c,1)&&c[1]===undefined, bits:bits(c[3]), units:[c.extra.charCodeAt(0)], descriptor:descriptor(c,'1')};
});
profile('getters', () => {
  const events=[], root={get a(){events.push('a'); delete this.b; Object.defineProperty(this,'c',{enumerable:false}); return -0},b:1,c:2};
  const c=structuredClone(root);
  return {events,keys:Object.keys(c),bits:bits(c.a),c:c.c,descriptor:descriptor(c,'a')};
});
profile('error', () => {
  const child={n:17}, e=new TypeError('\ud800X\udc00'); e.stack='\udc00trace'; e.cause={child,again:child}; e.extra=()=>{};
  const c=structuredClone(e);
  const result = {name:c.name,units:Array.from({length:c.message.length},(_,i)=>c.message.charCodeAt(i)),stack:Array.from({length:c.stack.length},(_,i)=>c.stack.charCodeAt(i)),alias:c.cause.child===c.cause.again,detached:c.cause!==e.cause,extra:Object.hasOwn(c,'extra'),keys:Object.keys(c),messageDescriptor:descriptor(c,'message'),stackFlags:[Object.getOwnPropertyDescriptor(c,'stack').enumerable,Object.getOwnPropertyDescriptor(c,'stack').configurable],causeDescriptor:descriptor(c,'cause')};
  c.stack='\ud800replacement'; result.stackWrite=Array.from({length:c.stack.length},(_,i)=>c.stack.charCodeAt(i)); return result;
});
profile('error-undefined', () => {
  const e=new Error();e.stack=17;e.cause=undefined;
  const c=structuredClone(e);
  const result = {message:Object.hasOwn(c,'message'),stack:Object.hasOwn(c,'stack')&&c.stack===undefined,cause:Object.hasOwn(c,'cause')&&c.cause===undefined,keys:Object.keys(c),stackFlags:[Object.getOwnPropertyDescriptor(c,'stack').enumerable,Object.getOwnPropertyDescriptor(c,'stack').configurable],causeDescriptor:descriptor(c,'cause')};
  c.stack=23; result.stackWrite=c.stack; return result;
});
profile('refusal', () => [()=>{}, Symbol.iterator].map(v=>{try {structuredClone(v);assert.fail('unexpected clone');} catch(e){return e.name}}));
profile('getter-throw', () => {
  const failure={original:true}; let calls=0;
  try{structuredClone({get value(){calls++;throw failure}});assert.fail('unexpected clone')}catch(e){return {identity:e===failure,calls}};
});
console.log(JSON.stringify(cases));
