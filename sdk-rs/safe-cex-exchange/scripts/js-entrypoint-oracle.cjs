'use strict';
const bits = n => { const b=Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const view = (v,token) => v===undefined?['undefined']:v===null?['null']:typeof v==='number'?['number',bits(v)]:typeof v==='string'?['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))]:typeof v==='boolean'?['boolean',v]:['reference',v===token];
const arrayView = (a,t) => Array.from({length:a.length},(_,i)=>i in a?view(a[i],t):['hole']);
const pair = k => {
 switch(k) {
  case 'nan':return [NaN,NaN]; case 'zero':return [-0,0];case 'units':return ['\ud800','\ud800'];case 'differentUnits':return ['\ud800','\ufffd'];case 'nullish':return [null,undefined];case 'boolean':return [true,true];case 'numberString':return [1,'1'];
  case 'array': {const a=[];return [a,a];}case 'otherArray':return [[],[]];
  case 'function':{const a=function f(){};return [a,a];}case 'otherFunction':return [function f(){},function f(){}];
  case 'map':{const a=new Map();return [a,a];}case 'set':{const a=new Set();return [a,a];}
  case 'object':{const a={};return [a,a];}case 'otherObject':return [{},{}];
  default:throw Error(k);
 }
};
function observe(method,mode) {
 if(method==='array'||method==='holey_array') {
  const token={},value=mode==='zero'?-0:mode==='units'?'\ud800':token;
  return arrayView(method==='array'?[value,undefined]:[,value,undefined],token);
 }
 if(method==='call') {
  const token={};const f=function(...args){if(args[0]===true)throw token;if(this!==token||args[0]!==token)throw Error('call identity');return args[1];};
  try{return ['ok',view(f.apply(token,mode==='throw'?[true]:[token,mode==='zero'?-0:'\ud800']),token)];}catch(e){return ['error',e===token];}
 }
 if(['strict_equals','same_value_zero','same_reference'].includes(method)) {
  const [a,b]=pair(mode);
  return method==='strict_equals'?a===b:method==='same_value_zero'?new Set([a]).has(b):(typeof a==='object'&&a!==null||typeof a==='function')&&a===b;
 }
 if(method==='ordinary_has_instance') {
  const token={},f=function F(){},p={};f.prototype=p;const v=mode==='miss'?{}:Object.create(p);let target=f;
  if(mode==='bound')target=f.bind(null);
  if(mode==='invalid')f.prototype=1;
  if(mode==='throw') {target=new Proxy(f,{get(t,k,r){if(k==='prototype')throw token;return Reflect.get(t,k,r);}});}
  if(mode==='noncallable')target={};
  try {return ['ok',Function.prototype[Symbol.hasInstance].call(target,mode==='primitive'?1:v)];}catch(e){return ['error',e===token];}
 }
 if(method==='set_key'||method==='define_key') {
  const key='\ud800x',o={},token={},calls=[];
  if(mode==='setter'||mode==='throw')Object.defineProperty(o,key,{configurable:true,set(v){calls.push(this===o&&v===token);if(mode==='throw')throw token;}});
  if(mode==='readonly')Object.defineProperty(o,key,{value:7});
  let status;try {if(method==='set_key')o[key]=mode==='zero'?-0:token;else Object.defineProperty(o,key,{value:mode==='zero'?-0:token,writable:true,enumerable:true,configurable:true});status=['ok'];}catch(e){status=['error',e===token];}
  return {status,value:view(o[key],token),calls,rawKey:Object.getOwnPropertyNames(o).includes(key)};
 }
 if(method==='set_write_observer') {
  if(mode==='array')return {install:false,calls:[],status:['none']};
  // Native publication hook: model completion-before-notification with a Proxy.
  const token={},target={},calls=[];
  const o=new Proxy(target,{set(t,k,v,r){const ok=Reflect.set(t,k,v,r);if(ok&&mode!=='clear'){calls.push([r===o,Array.from({length:k.length},(_,i)=>k.charCodeAt(i)),r[k]===v,v===token]);if(mode==='throw')throw token;}return ok;}});
  let status;try{o['\ud800']=token;status=['ok'];}catch(e){status=['error',e===token];}return {install:true,calls,status};
 }
 if(method==='push') {
  const token={},a=mode==='notArray'?null:[,undefined];if(mode==='readonly')Object.defineProperty(a,'length',{writable:false});let ok;try{a.push(mode==='zero'?-0:token);ok=true;}catch{ok=false;}return {ok,array:a===null?null:arrayView(a,token)};
 }
 if(method==='sort_array_with') {
  const token={},a=[3,,undefined,-0,2],trace=[];let ok=true,originalError=false;try{a.sort((x,y)=>{trace.push([x,y]);if(mode==='throw')throw token;return mode==='descending'?y-x:x-y;});}catch(e){ok=false;originalError=e===token;}return {ok,originalError,array:arrayView(a,token),trace};
 }
 if(method==='splice_array') {
  const token={},a=[-0,,undefined,'\ud800',token];if(mode==='grow')a.splice(1,0,token,-0);else if(mode==='shrink')a.splice(1,3);else if(mode==='replace')a.splice(0,2,undefined);else throw Error(mode);return arrayView(a,token);
 }
 throw Error(method);
}
const pairs=['nan','zero','units','differentUnits','nullish','boolean','numberString','array','otherArray','function','otherFunction','map','set','object','otherObject'];
const profiles={array:["zero","units","reference"],holey_array:["zero","units","reference"],call:["zero","units","throw"],sort_array_with:['ascending','descending','throw'],splice_array:['grow','shrink','replace'],ordinary_has_instance:['match','miss','primitive','bound','invalid','throw','noncallable'],strict_equals:pairs,same_value_zero:pairs,same_reference:pairs,set_write_observer:['normal','clear','throw','array'],set_key:['normal','zero','setter','throw','readonly'],define_key:['normal','zero','setter','throw','readonly'],push:['normal','zero','readonly','notArray']};
console.log(JSON.stringify(Object.entries(profiles).flatMap(([method,modes])=>modes.map(input=>({method,input,output:observe(method,input)})))));
