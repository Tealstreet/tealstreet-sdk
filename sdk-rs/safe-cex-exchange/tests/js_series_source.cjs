'use strict';
const lib = require('p-iteration');
const bits = value => {const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex');};
const describe = value => typeof value === 'number' ? {bits:bits(value)} : value === undefined ? {undefined:true} : value === null ? {null:true} : typeof value === 'string' ? {units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))} : typeof value === 'bigint' ? {bigint:String(value)} : {kind:typeof value};
const labels = ['dense','holes','promised-item','grow','shrink','delete','inherited','call-before-value','call-throws','value-throws','item-rejects','callback-rejects','this-undefined','this-false','this-zero','this-nan','this-object','return-reference','string','special-values','thenable-item','thenable-callback','then-getter-throws','then-rejects'];
(async () => {
 const rows=[];
 for (const method of ['mapSeries','forEachSeries']) for (const label of labels) {
  const trace=[], token={}, explicit={};
  const thenable=value=>{const object={get then(){trace.push('getThen');if(label==='then-getter-throws')throw token;return function(resolve,reject){trace.push({thenThis:this===object});if(label==='then-rejects'){reject(token);resolve(value);}else resolve(value);};}};return object;};
  let array=[7,8];
  if(['thenable-item','then-getter-throws','then-rejects'].includes(label)) array=[thenable(7)];
  if(label==='holes') {array=[];array.length=4;array[1]=8;}
  if(label==='promised-item') array=[Promise.resolve(7)];
  if(label==='item-rejects') array=[Promise.reject(token)];
  if(label==='return-reference') array=[token];
  if(label==='inherited') array=Object.assign(Object.create({0:7}),{length:1});
  if(['call-before-value','call-throws','value-throws'].includes(label)) array={length:1,get 0(){trace.push('value');if(label==='value-throws')throw token;return 7;}};
  if(label==='string') array='ab';
  if(label==='special-values') array=[NaN,-0,Infinity,-Infinity,123n,'\ud800'];
  const callback=function(value,index,source) {
   trace.push('callback'+index);
   if(label==='grow' && index===0) source.push(9);
   if(label==='shrink' && index===0) source.length=1;
   if(label==='delete' && index===0) delete source[1];
   if(label==='callback-rejects') return Promise.reject(token);
   trace.push({value:describe(value),index,sourceSame:source===array,thisModule:this===lib,thisExplicit:this===explicit});
   return label==='thenable-callback'?thenable(value):value;
  };
  Object.defineProperty(callback,'call',{get(){trace.push('getCall');if(label==='call-throws')throw token;return Function.prototype.call;}});
  const receiver=label==='this-object'?explicit:label==='this-false'?false:label==='this-zero'?0:label==='this-nan'?NaN:undefined;
  let output;
  try {
   const pending=lib[method](array,callback,receiver);trace.push('returned');
   const result=await pending;
   output=result===undefined?{undefined:true}:{length:result.length,slots:Array.from({length:result.length},(_,i)=>({own:Object.hasOwn(result,i),value:describe(result[i])})),same:label==='return-reference'?result[0]===token:null};
  } catch(error) {output=error===token?{original:true}:{error:error.name};}
  rows.push({method,label,trace,output});
 }
 console.log(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
