const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const source=JSON.parse(fs.readFileSync(path.join(__dirname,'typed-data.json'),'utf8')).cases[0];
const cases=[];
for(const shape of ['root-array','domain-array','field-array','type-map-array']) {
 let data=JSON.parse(source.raw);
 data.message.zPayload='3618502788666131213697322783095070105623107215331596699973092056135872020481';
 if(shape==='root-array')data=[data.types,data.domain,data.primaryType,data.message];
 if(shape==='domain-array')data.domain=Object.values(data.domain);
 if(shape==='field-array')data.types.Message=data.types.Message.map(field=>[field.name,field.type]);
 if(shape==='type-map-array')data.types=Object.values(data.types);
 const raw=JSON.stringify(data),row={name:shape,raw,address:source.address};
 try{row.hash=typedData.getMessageHash(JSON.parse(raw),source.address);}
 catch(error){row.error={name:error.name,message:error.message};}
 assert.ok(row.error,`SDK must refuse ${shape}`);cases.push(row);
}
const fixture=path.join(__dirname,'typed-shapes.json'),result=JSON.stringify({oracle:'pinned Stark SDK shape refusals with over-prime declared message',cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(fixture,result,{flag:'wx'});
else assert.equal(result,fs.readFileSync(fixture,'utf8'),'Stark shape oracle drift');
