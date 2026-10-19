// Installed Axios is the dependency used by the maintained source interceptors.
const assert=require('node:assert/strict');
const {AxiosHeaders}=require('axios');
const cases=[
 {defaults:{'Content-Type':'application/json','X-Trace':'first'},request:{'content-type':'text/plain','x-trace':'second'},sets:[['POLY_ADDRESS','wallet'],['poly_address','updated'],['POLY_TIMESTAMP',1700000000]]},
 {defaults:{A:' first\r\nvalue\0 ',B:'\t padded \t'},request:{},sets:[[' a ','\t final\r\nvalue\x7f '],['Empty',''],['Null',null],['Array',[' a ',2,null,false]]]},
 {defaults:{A:'a'},request:{},sets:[['a','kept',false],['A','forced',true],['Disabled',false],['disabled','ignored'],['DISABLED','forced',true]]},
 {defaults:{},request:{},sets:[[{One:1,Two:'two'}],['one','no',false],['two','yes'],['','ignored'],[null,'ignored']]},
];
const probes=['content-type','x-TRACE','POLY_ADDRESS','poly_timestamp','A','B','Empty','Null','Array','Disabled','One','Two','missing'];
for(const c of cases){
 const h=AxiosHeaders.concat(c.defaults,c.request);
 c.snapshots=[];
 for(const args of c.sets){assert.equal(h.set(...args),h,'source fluent receiver');c.snapshots.push({entries:Object.entries(h),keys:Object.keys(h),probes:probes.map(name=>({name,present:h.has(name),value:h.get(name)}))});}
}
const coercions=[];
for(const kind of ['string','number','fallback','noncallable','throws','both-object']) {
 for(const method of ['get','has']) {
  const trace=[], thrown={marker:'header-name'};
  const name={
   toString(){trace.push('toString');if(kind==='throws')throw thrown;if(kind==='number')return 123;if(kind==='string')return '  X-Custom  ';return {};},
   valueOf(){trace.push('valueOf');return kind==='both-object'?{}:'  X-Custom  ';},
  };
  if(kind==='noncallable')name.toString=false;
  const headers=new AxiosHeaders({'123':'numeric','X-Custom':'custom'});
  let result;
  try {
   const value=headers[method](name);
   result=typeof value==='string'?{kind:'string',units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))}:{kind:typeof value,value};
  } catch(error) {result={kind:'throw',sameThrown:error===thrown,name:error===thrown?null:error.name};}
  coercions.push({kind,method,trace,result});
 }
}
assert.equal(coercions.length,12);
const fs=require('node:fs'), path=require('node:path');
const headerSource=fs.readFileSync(path.join(path.dirname(require.resolve('axios/package.json')),'lib/core/AxiosHeaders.js'),'utf8');
const declaration=headerSource.match(/^function normalizeHeader\(header\) \{\n[^]*?^\}/m);
assert.ok(declaration,'installed Axios normalizeHeader source declaration');
const normalizeHeader=Function(`return (${declaration[0]})`)();
const normalizationInputs=[
 {kind:'text',units:[32,88,45,84,101,115,116,9]},
 {kind:'text',units:[0xd800,65]},
 {kind:'text',units:[0x130]},
 {kind:'number',bits:'8000000000000000'},
 {kind:'number',bits:'405ec00000000000'},
 {kind:'boolean',value:true},
 {kind:'boolean',value:false},
 {kind:'null'}, {kind:'undefined'},
];
const normalizations=normalizationInputs.map(input=>{
 let value;
 if(input.kind==='text')value=String.fromCharCode(...input.units);
 else if(input.kind==='number'){const bytes=Buffer.alloc(8);bytes.writeBigUInt64BE(BigInt('0x'+input.bits));value=bytes.readDoubleBE();}
 else if(input.kind==='boolean')value=input.value;
 else if(input.kind==='null')value=null;
 const normalized=normalizeHeader(value);
 return {input,output:normalized?Array.from({length:normalized.length},(_,i)=>normalized.charCodeAt(i)):[]};
});
assert.equal(normalizations.length,9);
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),cases,coercions,normalizations}));
