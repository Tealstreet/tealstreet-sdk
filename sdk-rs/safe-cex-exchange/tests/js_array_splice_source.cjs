const assert = require('node:assert/strict');
const values = {undefined:undefined, nan:NaN, minusInfinity:-Infinity, infinity:Infinity,
 null:null, zero:0, negative:-2, fraction:1.9, large:100};
const view = a => ({length:a.length,slots:Array.from({length:a.length},(_,i)=>
 i in a ? [true, String(a[i])] : [false,null])});
const cases = [];
for (const [start,a] of Object.entries(values)) for (const [count,b] of Object.entries(values)) {
 for (const insert of [false,true]) {
  const array = [0,,2,3];
  const args = [a,b,...(insert?['x','y']:[])];
  const deleted = array.splice(...args);
  cases.push({start,count,insert,array:view(array),deleted:view(deleted)});
 }
}
for (const args of [[],[undefined],[-2]]) {
 const array=[0,,2,3]; const deleted=array.splice(...args);
 cases.push({arity:args.length,start:args.length?(args[0]===undefined?'undefined':'negative'):null,
 array:view(array),deleted:view(deleted)});
}
const reentrant=[];
for (const where of ['start','deleted']) {
 const a=[0,1,2,3], trace=[];
 let start=1;
 if(where==='start') start={valueOf(){trace.push('start');a.length=1;return 1}};
 else Object.defineProperty(a,'1',{get(){trace.push('deleted');a.length=2;return 8}, configurable:true});
 const removed=a.splice(start,1);
 reentrant.push({where,array:view(a),deleted:view(removed),trace});
}
assert.equal(cases.length,165);
assert.deepEqual(reentrant.map(row=>row.array.length),[3,3]);
process.stdout.write(JSON.stringify({cases,reentrant}));
