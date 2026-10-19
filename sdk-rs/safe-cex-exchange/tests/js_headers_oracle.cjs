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
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),cases,coercions}));
