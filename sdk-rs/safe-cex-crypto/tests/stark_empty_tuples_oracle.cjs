const assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('./fixtures/oracle-packages.cjs');verifyPackage('starknet');
const {typedData}=oracleRequire('starknet');
const shapes=['scalar','nested','array','enum','merkle1','merkle2','merkle3'],variants=['Zero','Last'],cases=[];
for(const shape of shapes)for(const variant of variants){
 const types={StarknetDomain:[{name:'name',type:'shortstring'},{name:'version',type:'shortstring'},{name:'chainId',type:'shortstring'},{name:'revision',type:'shortstring'}],Choice:[{name:'Zero',type:'()'},{name:'Scalar',type:'(felt)'},{name:'Last',type:'()'}],Wrapper:[{name:'lead',type:'felt'},{name:'choice',type:'enum',contains:'Choice'},{name:'tail',type:'felt'}]};
 const wrapped=(n,v=variant)=>({lead:String(101+n),choice:{[v]:[]},tail:String(509+n)});let message;
 if(shape==='scalar'){types.Message=[{name:'choice',type:'enum',contains:'Choice'},{name:'marker',type:'felt'}];message={choice:{[variant]:[]},marker:'73'};}
 else if(shape==='nested'){types.Message=[{name:'inner',type:'Wrapper'},{name:'marker',type:'felt'}];message={inner:wrapped(3),marker:'79'};}
 else if(shape==='array'){types.Message=[{name:'items',type:'Wrapper*'},{name:'marker',type:'felt'}];message={items:[wrapped(7),wrapped(13,variant==='Zero'?'Last':'Zero')],marker:'83'};}
 else if(shape==='enum'){types.Envelope=[{name:'Other',type:'(felt)'},{name:'Wrapped',type:'(Wrapper)'},{name:'Direct',type:'()'}];types.Message=[{name:'outer',type:'enum',contains:'Envelope'},{name:'marker',type:'felt'}];message={outer:{Wrapped:[wrapped(17)]},marker:'89'};}
 else{const count=Number(shape.slice(-1));types.Message=[{name:'tree',type:'merkletree',contains:'Wrapper'},{name:'marker',type:'felt'}];message={tree:Array.from({length:count},(_,i)=>wrapped(23+i)),marker:'97'};}
 const data={types,domain:{name:'EmptyTupleFixture',version:'1',chainId:'TEST',revision:'1'},primaryType:'Message',message};
 const raw=JSON.stringify(data),hash=typedData.getMessageHash(data,'0x1234');cases.push({shape,variant,raw,address:'0x1234',hash});
}
assert.equal(cases.length,14);assert.equal(new Set(cases.map(row=>row.hash)).size,14);
process.stdout.write(JSON.stringify({shapes,variants,cases}));
