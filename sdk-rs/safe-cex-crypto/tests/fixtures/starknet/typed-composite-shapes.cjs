const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('starknet');const {typedData}=oracleRequire('starknet');
const base=JSON.parse(JSON.parse(fs.readFileSync(path.join(__dirname,'typed-data.json'),'utf8')).cases[1].raw),cases=[];
for(const token of ['0','true','[]','3618502788666131213697322783095070105623107215331596699973092056135872020481']) {
 const data=JSON.parse(JSON.stringify(base));data.types.Message=[{name:'x',type:'Leaf'}];data.types.Leaf=[{name:'$serde_json::private::Number',type:'felt'}];data.message={x:'RAW_NUMBER'};
 const raw=JSON.stringify(data).replace('"RAW_NUMBER"',token),row={name:'custom-record/'+token,raw,address:'0x1234'};
 try{row.hash=typedData.getMessageHash(JSON.parse(raw),row.address);}catch(error){row.error={name:error.name,message:error.message};}
 assert.ok(row.error,'SDK requires the declared custom-record fields');cases.push(row);
}
const target=path.join(__dirname,'typed-composite-shapes.json'),result=JSON.stringify({oracle:'pinned Stark SDK refusal of scalar/array custom records, including raw arbitrary-precision text',cases},null,2)+'\n';if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'Stark composite-shape oracle drift');
