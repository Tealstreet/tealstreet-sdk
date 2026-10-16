const fs=require('node:fs'),path=require('node:path');
const {oracleRequire,verifyPackage}=require('../tests/fixtures/oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const base=JSON.parse(JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/fixtures/starknet/typed-data.json'),'utf8')).cases[1].raw);
const p='3618502788666131213697322783095070105623107215331596699973092056135872020481';const cases=[];
for(const tuple of ['()','(felt)','(felt,felt)'])for(const values of [[],['1'],['1','2'],['1','2',p]]){
 const data=structuredClone(base);data.types.Message=[{name:'choice',type:'enum',contains:'Choice'}];data.types.Choice=[{name:'Chosen',type:tuple}];data.message={choice:{Chosen:values}};
 const raw=JSON.stringify(data);const row={tuple,values,raw,address:'0x1234'};
 try{row.hash=typedData.getMessageHash(data,row.address)}catch(error){row.error={name:error.name,message:error.message}}cases.push(row);
}
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,nodeVersion:process.version,cases}));
