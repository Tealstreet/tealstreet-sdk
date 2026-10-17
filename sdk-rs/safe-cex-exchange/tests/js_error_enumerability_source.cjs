const assert = require('node:assert/strict');
const units = value => Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
const inputs = {
 omitted:()=>[], undefined:()=>[undefined], null:()=>[null], empty:()=>[''], text:()=>['failure'],
 number:()=>[17], negative_zero:()=>[-0], nan:()=>[NaN], infinity:()=>[Infinity],
 surrogate:()=>['\ud800'], object:()=>[{toString(){return 'coerced'}}],
 throwing_object:()=>[{toString(){throw new RangeError('conversion')}}], symbol:()=>[Symbol.iterator],
};
const descriptor = (error,key) => {
 const d=Object.getOwnPropertyDescriptor(error,key);
 return d?{value:units(d.value),writable:d.writable,enumerable:d.enumerable,configurable:d.configurable}:null;
};
const cases=[];
for(const ctor of ['Error','SyntaxError','TypeError'])for(const [tag,input] of Object.entries(inputs))for(const op of ['pristine','message','name']){
 let result;
 try {
  const error=globalThis[ctor](...input());
  if(op==='message')error.message='changed';
  if(op==='name')error.name='UserError';
  result={name:units(error.name),message:units(error.message),keys:Object.keys(error),json:JSON.stringify(error),copy:JSON.stringify(Object.assign({},error)),nameDescriptor:descriptor(error,'name'),messageDescriptor:descriptor(error,'message'),render:units(String(error))};
 }catch(error){result={thrown:error.name};}
 cases.push({ctor,tag,op,result});
}
assert.equal(cases.length,117);
const axios = new (require("axios").AxiosError)("failure");
process.stdout.write(JSON.stringify({cases,axios:{nameDescriptor:descriptor(axios,"name"),messageDescriptor:descriptor(axios,"message")}}));
