const assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('./fixtures/oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const spellings=['1','+1','0X1','0b1','0B1','0o1','0O1',' 1 ','\u00a01\ufeff','', ' ','\ufeff','+0x1','-0x1','+0b1','1e0','1.0','1_0','\u00851','-0','+0','0x00','01','0O7','0B101','+001','-1'];
const rows=[];
for(const revision of [0,1])for(const kind of ['felt','shortstring'])for(const spelling of spellings){
 const name=revision?'StarknetDomain':'StarkNetDomain',domainKind=revision?'shortstring':'felt';
 const data={types:{[name]:[{name:'name',type:domainKind},{name:'version',type:domainKind},{name:'chainId',type:domainKind},...(revision?[{name:'revision',type:'shortstring'}]:[])],Message:[{name:'x',type:kind}]},primaryType:'Message',domain:{name:'Fixture',version:'1',chainId:'TEST',...(revision?{revision:'1'}:{})},message:{x:spelling}};
 const raw=JSON.stringify(data),row={revision,kind,spelling,raw,address:'0x1234'};
 try{row.hash=typedData.getMessageHash(data,row.address);}catch(error){row.error={name:error.name,message:error.message};}
 rows.push(row);
}
assert.equal(rows.length,108);
const prime=(1n<<251n)+17n*(1n<<192n)+1n, range=[];
const alternatives=['+'+prime,'0X'+prime.toString(16),'0B'+prime.toString(2),'0O'+prime.toString(8),'\ufeff'+prime+'\u00a0'];
for(const revision of [0,1])for(const shape of ['scalar','nested','array','domain'])for(const spelling of alternatives){
 const domain=revision?'StarknetDomain':'StarkNetDomain',kind=revision?'shortstring':'felt';
 const data={types:{[domain]:[{name:'name',type:kind},{name:'version',type:kind},{name:'chainId',type:kind},...(revision?[{name:'revision',type:'shortstring'}]:[])],Message:[{name:'x',type:shape==='array'?'felt*':shape==='nested'?'Inner':'felt'}],...(shape==='nested'?{Inner:[{name:'n',type:'felt'}]}:{})},primaryType:'Message',domain:{name:'Fixture',version:'1',chainId:'TEST',...(revision?{revision:'1'}:{})},message:{x:shape==='array'?[spelling]:shape==='nested'?{n:spelling}:spelling}};
 if(shape==='domain'){data.domain.name=spelling;data.message.x='7';}
 const row={revision,shape,spelling,raw:JSON.stringify(data),address:'0x1234'};
 try{row.hash=typedData.getMessageHash(data,row.address);}catch(error){row.error={name:error.name,message:error.message};}
 assert.ok(row.error,'SDK rejects alternative prime spelling');range.push(row);
}
assert.equal(range.length,40);const accounts=[];
for(const address of ['0x0','0x'+(prime-1n).toString(16),'0x'+prime.toString(16),'0x'+(prime+1n).toString(16)]){
 const row={address,raw:rows[0].raw};try{row.hash=typedData.getMessageHash(JSON.parse(row.raw),address);}catch(error){row.error={name:error.name,message:error.message};}accounts.push(row);
}
assert.equal(accounts.length,4);assert.ok(accounts[2].error);assert.ok(accounts[3].error);
const permutations=[['name','version','chainId'],['name','chainId','version'],['version','name','chainId'],['version','chainId','name'],['chainId','name','version'],['chainId','version','name']], domains=[];
for(const fields of permutations){const data={types:{StarkNetDomain:fields.map(name=>({name,type:'felt'})),Message:[{name:'x',type:'felt'}]},domain:{name:'Name17',version:'Version29',chainId:'Chain41',ignored:null},primaryType:'Message',message:{x:'+1'}};const raw=JSON.stringify(data);domains.push({fields,raw,address:'0x1234',hash:typedData.getMessageHash(data,'0x1234')});}
assert.equal(new Set(domains.map(d=>d.hash)).size,6);
process.stdout.write(JSON.stringify({spellings,rows,range,accounts,domains}));
