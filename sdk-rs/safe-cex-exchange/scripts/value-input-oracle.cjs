'use strict';
const pairs = ['nan','nan-payload','nan-sign','zero','minus-zero','plus-zero','infinity','infinity-sign','number','other-number','nullish','undefined','null','boolean','other-boolean','boolean-number','number-string','units','other-units','high-low','bigint','other-bigint','bigint-number','symbol','object','other-object','array','other-array','function','other-function','map','other-map','set','other-set'];
function pair(mode) {
  switch (mode) {
    case 'nan': return [NaN,NaN];
    case 'nan-payload': return [Buffer.from('7ff8000000000001','hex').readDoubleBE(),Buffer.from('7ff8000000000002','hex').readDoubleBE()];
    case 'nan-sign': return [NaN,-NaN];
    case 'zero': return [-0,0];
    case 'minus-zero': return [-0,-0];
    case 'plus-zero': return [0,0];
    case 'infinity': return [Infinity,Infinity];
    case 'infinity-sign': return [Infinity,-Infinity];
    case 'number': return [1,1];
    case 'other-number': return [1,2];
    case 'nullish': return [null,undefined];
    case 'undefined': return [undefined,undefined];
    case 'null': return [null,null];
    case 'boolean': return [true,true];
    case 'other-boolean': return [true,false];
    case 'boolean-number': return [true,1];
    case 'number-string': return [1,'1'];
    case 'units': return ['\ud800','\ud800'];
    case 'other-units': return ['\ud800','\ufffd'];
    case 'high-low': return ['\ud800','\udc00'];
    case 'bigint': return [17n,17n];
    case 'other-bigint': return [17n,18n];
    case 'bigint-number': return [17n,17];
    case 'symbol': return [Symbol.iterator,Symbol.iterator];
    default: {
      const make = () => ({object:()=>({}),array:()=>[],function:()=>function f(){},map:()=>new Map(),set:()=>new Set()})[mode.replace(/^other-/,'')]();
      const left=make(); return [left,mode.startsWith('other-')?make():left];
    }
  }
}
const bits = n => {const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const nativeView = (v,token) => v===undefined?['undefined']:v===null?['null']:typeof v==='boolean'?['boolean',v]:typeof v==='number'?(Number.isNaN(v)?['nan']:['number',bits(v)]):typeof v==='string'?['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))]:typeof v==='bigint'?['bigint',String(v)]:typeof v==='symbol'?['symbol']:['reference',v===token];
const argument = index => {
  const token={},array=[],fn=function token(){},values=[undefined,null,-0,42,NaN,'\ud800\udc00\udfff',token,array,fn,17n];
  const value=(function(){return arguments[index];})(...values);
  const view=value===undefined?['undefined']:value===null?['null']:typeof value==='number'?(Number.isNaN(value)?['nan']:['number',bits(value)]):typeof value==='string'?['string',Array.from({length:value.length},(_,i)=>value.charCodeAt(i))]:typeof value==='bigint'?['bigint',String(value)]:['reference',value===token,value===array,value===fn];
  return {ok:true,value:view,originalReference:(typeof value==='object'&&value!==null||typeof value==='function')&&value===values[index]};
};
const rows=pairs.map(input=>{const [left,right]=pair(input);return {method:'same_value',input,output:{ok:true,equal:Object.is(left,right),left:nativeView(left,left),right:nativeView(right,left)}};});
for (const input of ['none','missing-left','missing-right',...pairs]) {
  const [left,right]=input==='none'?[{},{}]:input==='missing-left'?[{},{value:undefined}]:input==='missing-right'?[{value:undefined},{}]:pair(input).map(value=>({value}));
  const leftPresent=Object.hasOwn(left,'value'),rightPresent=Object.hasOwn(right,'value');
  rows.push({method:'same_optional_value',input,output:{ok:true,equal:leftPresent===rightPresent&&(!leftPresent||Object.is(left.value,right.value)),leftPresent,rightPresent}});
}
for(const input of [0,1,2,3,4,5,6,7,8,9,10,9007199254740991])rows.push({method:'arg',input,output:argument(input)});
console.log(JSON.stringify(rows));
