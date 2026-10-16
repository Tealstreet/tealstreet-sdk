const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const bases=JSON.parse(fs.readFileSync(path.join(__dirname,'typed-data.json'),'utf8')).cases;
const prime='3618502788666131213697322783095070105623107215331596699973092056135872020481', cases=[];
function capture(name,data,address='0x1234') {const raw=JSON.stringify(data),row={name,raw,address};try{row.hash=typedData.getMessageHash(JSON.parse(raw),address);}catch(error){row.error={name:error.name,message:error.message};}cases.push(row);}
for(const revision of [0,1]) {
 for(const kind of ['felt','shortstring','ContractAddress','ClassHash']) for(const zero of ['-0','-00']) {
  const data=JSON.parse(bases[revision].raw);data.types.Message=[{name:'x',type:kind}];data.message={x:zero};capture(`rev${revision}/${kind}/negative-zero-${zero}`,data);
 }
 for(const value of [prime,'-0']) {
  const data=JSON.parse(bases[revision].raw);data.domain.name=value;capture(`rev${revision}/domain/${value===prime?'p':'negative-zero'}`,data);
  const tree=JSON.parse(bases[revision].raw);tree.types.Message=[{name:'tree',type:'merkletree',contains:'Leaf'}];tree.types.Leaf=[{name:'x',type:'felt'}];tree.message={tree:[{x:value},{x:'7'}]};capture(`rev${revision}/merkletree/${value===prime?'p':'negative-zero'}`,tree);
  if(revision===1) {const data=JSON.parse(bases[revision].raw);data.types.Message=[{name:'choice',type:'enum',contains:'Choice'}];data.types.Choice=[{name:'Chosen',type:'(felt)'}];data.message={choice:{Chosen:[value]}};capture(`rev1/enum/${value===prime?'p':'negative-zero'}`,data);}
 }
}
const fixture=path.join(__dirname,'typed-compositions.json'),result=JSON.stringify({oracle:'pinned Stark SDK negative-zero and domain/enum/merkletree range edges',cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(fixture,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(fixture,'utf8'),'Stark composition boundary drift');
for(const row of cases)assert.ok(row.hash||row.error,row.name);
