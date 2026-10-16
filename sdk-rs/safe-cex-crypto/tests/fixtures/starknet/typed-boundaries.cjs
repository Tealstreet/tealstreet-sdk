// Oracle first: execute the pinned SDK, never reproduce Rust's encoder.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const p=(1n<<251n)+17n*(1n<<192n)+1n, cases=[];
function capture(name,data) {
 const raw=JSON.stringify(data),address='0x1234';
 const row={name,raw,address};
 try {row.hash=typedData.getMessageHash(JSON.parse(raw),address);}
 catch(error) {row.error={name:error.name,message:error.message};}
 cases.push(row);
}
function data(revision,type,value,shape='scalar') {
 const domain=revision?'StarknetDomain':'StarkNetDomain';
 const types={[domain]:[{name:'name',type:revision?'shortstring':'felt'},{name:'version',type:revision?'shortstring':'felt'},{name:'chainId',type:revision?'shortstring':'felt'},...(revision?[{name:'revision',type:'shortstring'}]:[])],Message:[{name:'x',type}]};
 let message={x:value};
 if(shape==='nested') {types.Inner=[{name:'x',type}];types.Message=[{name:'inner',type:'Inner'}];message={inner:{x:value}};}
 if(shape==='array') {types.Message=[{name:'x',type:type+'*'}];message={x:[value]};}
 return {types,primaryType:'Message',domain:{name:'Fixture',version:'1',chainId:'TEST',...(revision?{revision:'1'}:{})},message};
}
for(const revision of [0,1]) for(const type of ['felt','shortstring','ContractAddress','ClassHash']) {
 for(const [label,value] of [['zero','0'],['one','1'],['p-minus-one',(p-1n).toString()],['p',p.toString()],['p-plus-one',(p+1n).toString()],['hex-p','0x'+p.toString(16)],['negative','-1']])
  capture(`rev${revision}/${type}/${label}`,data(revision,type,value));
 for(const shape of ['nested','array']) capture(`rev${revision}/${type}/${shape}-p`,data(revision,type,p.toString(),shape));
}
for(const value of [-5,'-5',0,'0',5,'5']) capture(`rev1/i128/${JSON.stringify(value)}`,data(1,'i128',value));
const extra=data(1,'felt','7');extra.message.ignored='not authenticated';capture('rev1/extra-undeclared',extra);
const fixture=path.join(__dirname,'typed-boundaries.json'),result=JSON.stringify({oracle:'pinned starknet@7.6.4 typedData.getMessageHash, prime-boundary refusals before reduction',cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(fixture,result,{flag:'wx'});
else assert.equal(result,fs.readFileSync(fixture,'utf8'),'Stark boundary oracle drift');
console.log(`Stark SDK boundary oracle: ${cases.length} cases`);
