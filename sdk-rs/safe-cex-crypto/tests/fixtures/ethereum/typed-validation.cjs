const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('ethers');
const ethers=oracleRequire('ethers'),key='0x'+'11'.repeat(32),cases=[];
const address='0x8ba1f109551bd432803012645ac136ddd64dba72';
function capture(name,domain,types,value) {
 const raw=JSON.stringify({domain,types:{EIP712Domain:[],...types},primaryType:'Message',message:value}),row={name,raw,key};
 try {row.hash=ethers.TypedDataEncoder.hash(domain,types,value);row.signature=new ethers.SigningKey(key).sign(row.hash).serialized;}
 catch(error){row.error={code:error.code,shortMessage:error.shortMessage};}cases.push(row);
}
const normal={name:'oracle',version:'1',chainId:1,verifyingContract:address};
for (const [name,domain] of [['normal',normal],['salt-only',{salt:'0x'+'12'.repeat(32)}],['empty',{}],['unknown-domain',{...normal,custom:'ignored?'}],['bad-domain-checksum',{...normal,verifyingContract:'0x8Ba1f109551bD432803012645Ac136ddd64DBA72'}]]) capture(name,domain,{Message:[{name:'x',type:'uint256'}]},{x:'7'});
capture('duplicate-field',normal,{Message:[{name:'x',type:'uint256'},{name:'x',type:'uint256'}]},{x:'7'});
for(const [name,addr] of [['lower-address',address],['checked-address',ethers.getAddress(address)],['wrong-checksum','0x8Ba1f109551bD432803012645Ac136ddd64DBA72'],['icap-address',ethers.getIcapAddress(address)]]) {
 capture(name,normal,{Message:[{name:'who',type:'address'}]},{who:addr});
 capture('nested-'+name,normal,{Message:[{name:'people',type:'Person[]'}],Person:[{name:'wallets',type:'address[][]'}]},{people:[{wallets:[[addr]]}]});
}
capture('nested-bytes-string',normal,{Message:[{name:'child',type:'Child'}],Child:[{name:'bytes',type:'bytes'},{name:'text',type:'string'},{name:'values',type:'uint256[][]'}]},{child:{bytes:'0x001122',text:'🙂'.repeat(100),values:[['0','9007199254740993']]}});
const target=path.join(__dirname,'typed-validation.json'),result=JSON.stringify({oracle:'pinned ethers live declared-domain, struct and address validation',cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'ethers typed validation drift');
