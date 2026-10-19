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
const valueProfiles=['sparse','custom-map','map-throws','map-noncallable','nested'];
const valueRows=valueProfiles.map(kind=>{
 const trace=[], token={marker:'array-map'};
 let input;
 if(kind==='sparse'){input=new Array(4);input[1]=' a ';input[3]=undefined;}
 if(kind==='nested')input=[false,null,undefined,['\t x\r\n ',123,'\ud800']];
 if(kind==='custom-map') {input=[1];input.map=function(fn){trace.push(this===input?'receiver':'wrong');return [fn(' X '),fn(false)];};}
 if(kind==='map-throws'){input=[1];Object.defineProperty(input,'map',{get(){trace.push('get-map');throw token;}});}
 if(kind==='map-noncallable'){input=[1];input.map=0;}
 const view=value=>Array.isArray(value)?['array',value.length,Object.keys(value),Array.from({length:value.length},(_,i)=>Object.hasOwn(value,i)?view(value[i]):['hole'])]:
  typeof value==='string'?['string',Array.from({length:value.length},(_,i)=>value.charCodeAt(i))]:[value===null?'null':typeof value,value===null||value===undefined?null:value];
 let output;try{const h=new AxiosHeaders({X:input});output={ok:true,value:view(h.get('x')),trace};}
 catch(error){output={ok:false,name:error===token?'original':error.name,trace};}
 return {input:kind,output};
});
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),cases,valueRows}));
