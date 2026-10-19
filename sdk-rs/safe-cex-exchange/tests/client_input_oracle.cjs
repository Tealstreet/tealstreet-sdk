'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {execFileSync}=require('node:child_process');
const axios=require('axios');
const {AxiosHeaders}=axios;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const view=v=>Array.isArray(v)?['array',v.length,Object.keys(v),Array.from({length:v.length},(_,i)=>Object.hasOwn(v,i)?view(v[i]):['hole'])]:
 typeof v==='string'?['string',units(v)]:typeof v==='number'?['number',(()=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex');})()]:[v===null?'null':typeof v,v==null?null:v];
const entries=h=>Object.entries(h).map(([k,v])=>[units(k),view(v)]);
const headers=JSON.parse(execFileSync(process.execPath,[path.join(__dirname,'js_headers_oracle.cjs')],{encoding:'utf8'}));
const values=headers.valueRows;
const from=['scalar','array','empty'].map(input=>{
 const value=input==='scalar'?{X:-0,Null:null,Off:false,Utf:'\ud800'}:input==='array'?{X:[123,null,false]}:undefined;
 // Native helper is the constructor adapter used by create_no_proxy, not AxiosHeaders.from's identity overload.
 const h=new AxiosHeaders(value);
 return {input,output:{entries:entries(h),get:typeof h.get==='function',has:typeof h.has==='function',same:h===value}};
});
const utilSource=fs.readFileSync(path.join(path.dirname(require.resolve('axios/package.json')),'lib/utils.js'),'utf8');
const declaration=utilSource.match(/^function findKey\(obj, key\) \{\n[^]*?^\}/m);
assert.ok(declaration);
const findKey=Function('isBuffer',`return (${declaration[0]})`)(()=>false);
const key=['hit','miss','falsy'].map(input=>{
 let readValue=false;
 const h={X:'first',x:'second',Other:'third'};
 Object.defineProperty(h,'Unread',{enumerable:true,get(){readValue=true;throw Error('unread header value');}});
 const name=input==='hit'?' X ':input==='miss'?'missing':-0;
 const normalized=name&&String(name).trim().toLowerCase();
 const selected=normalized?findKey(h,normalized):null;
 return {input,output:{key:selected==null?null:units(selected),readValue}};
});
const set=['default','true','false'].map(input=>{
 const h=new AxiosHeaders({X:false});
 h.set(' x ',' new\r\n ',input==='default'?undefined:input==='true');
 h.set('Empty',null);
 return {input,output:{entries:entries(h)}};
});
// These two helpers implement the typed provider model's comparison/status seam.
// Inputs here are explicit native model cells; they are not wire decoders.
const status=['undefined','null','false','negative-zero','nan','infinity','text','number','string-status'].map(input=>{
 const status={undefined:undefined,null:null,false:false,'negative-zero':-0,nan:NaN,infinity:Infinity,text:'\ud800',number:429,'string-status':'429'}[input];
 return {input,output:typeof status==='number'&&status!==0&&!Number.isNaN(status)?view(status)[1]:null};
});
const same=['equal','different','zero','undefined-null','nonfinite'].map(input=>{
 const [a,b]={equal:[{x:1},{x:1}],different:[{x:1},{x:2}],zero:[-0,0],'undefined-null':[undefined,null],nonfinite:[NaN,null]}[input];
 return {input,output:(a===undefined)===(b===undefined)&&require('node:util').isDeepStrictEqual(typeof a==='number'&&!Number.isFinite(a)?null:a,typeof b==='number'&&!Number.isFinite(b)?null:b)};
});
(async()=>{
 const {pathToFileURL}=require('node:url');
 const axiosRoot=path.dirname(require.resolve('axios/package.json'));
 const buildURL=(await import(pathToFileURL(path.join(axiosRoot,'lib/helpers/buildURL.js')).href)).default;
 const params=[];
 for(const input of ['function','object','throws','missing','noncallable']) {
  const trace=[],token={};let serializer;
  function serialize(value,options) {
   trace.push(this===undefined?'undefined':'owner');
   trace.push(options===serializer?'same-options':options?.serialize===serialize?'wrapped-options':'wrong-options');
   if(value.throws)throw token;return '\ud800&x';
  }
  serializer=input==='function'?serialize:input==='object'||input==='throws'?{serialize}:input==='noncallable'?{serialize:false}:undefined;
  let result;
  if(input==='missing'||input==='noncallable')result={value:view(input==='missing'?undefined:false)};
  else {
   try {const url=buildURL('/probe',{throws:input==='throws'},serializer);result={ok:true,value:view(url.slice('/probe?'.length))};}
   catch(error){result={ok:false,original:error===token};}
  }
  params.push({input,output:{callable:input!=='missing'&&input!=='noncallable',result,trace}});
 }
 const pipeline=[];
 for(const input of ['fulfilled','recover','pass-rejection']) {
  const trace=[],token={marker:'pipeline'};
  const xhr=axios.create({adapter:async config=>{trace.push('adapter');return {status:200,data:token,headers:{},config};}});
  xhr.interceptors.request.use(config=>{trace.push('inner');return config;});
  xhr.interceptors.request.use(config=>{trace.push('outer');if(input!=='fulfilled')throw token;return config;});
  if(input==='recover')xhr.interceptors.response.use(undefined,error=>{trace.push(error===token?'original-rejection':'wrong-rejection');return token;});
  xhr.interceptors.response.use(response=>{trace.push('response');return input==='fulfilled'?response.data:response;});
  let output;try {const value=await xhr.get('/probe');output={ok:true,original:value===token,trace};}
  catch(error){output={ok:false,original:error===token,trace};}
  pipeline.push({input,output});
 }
 process.stdout.write(JSON.stringify({from,key,normalize:values,set,params,status,same,pipeline}));
})().catch(error=>{console.error(error);process.exitCode=1;});
