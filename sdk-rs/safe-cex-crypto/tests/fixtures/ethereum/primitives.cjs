const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('ethers');
const ethers=oracleRequire('ethers');
const cases=[],addresses=[];
const user='0x52908400098527886e0f7030069857d2e4169ee7',signer='0xde709f2102306220921060314715629080e2fb77';
const privateKey='0x'+'11'.repeat(32);
for (const [name,payload,nonce] of [
 ['empty','',0n],['ascii','{"amount":"1"}',1n],['unicode-nul','🙂\0é',9007199254740993n],
 ['word-boundary','x'.repeat(32),1n<<255n],['word-plus-one','x'.repeat(33),(1n<<256n)-1n],
 ['long','z'.repeat(1024),999n],
]) {
 const abi=ethers.AbiCoder.defaultAbiCoder().encode(['string','address','address','uint256'],[payload,user,signer,nonce]);
 const keccak=ethers.keccak256(abi),messageHash=ethers.hashMessage(ethers.getBytes(keccak));
 const signature=new ethers.SigningKey(privateKey).sign(messageHash).serialized;
 cases.push({name,payload,user,signer,nonceHex:ethers.toBeHex(nonce,32),privateKey,abi,keccak,messageHash,signature,recovered:ethers.recoverAddress(messageHash,signature)});
}
const bytesCases=[0,1,31,32,33,256].map(length=>{const bytes='0x'+Array.from({length},(_,i)=>(i%256).toString(16).padStart(2,'0')).join('');return {name:`bytes-${length}`,bytes,keccak:ethers.keccak256(bytes),messageHash:ethers.hashMessage(ethers.getBytes(bytes))};});
const checked=ethers.getAddress(user),icap=ethers.getIcapAddress(user);
for(const [name,input] of [
 ['lower',user],['upper','0x'+user.slice(2).toUpperCase()],['checksum',checked],['no-prefix',user.slice(2)],
 ['invalid-checksum',checked.replace('E','e')],['short','0x1234'],['upper-prefix','0X'+user.slice(2)],
 ['bom','\ufeff'+user],['leading-space',' '+user],['trailing-space',user+' '],['invalid-hex',user.slice(0,-1)+'g'],
 ['icap',icap],['icap-lower',icap.toLowerCase()],['invalid-icap-checksum',icap.slice(0,2)+'00'+icap.slice(4)],
]) {const row={name,input};try{row.address=ethers.getAddress(input);}catch(error){row.error={code:error.code,shortMessage:error.shortMessage};}addresses.push(row);}
assert.ok(addresses.find(v=>v.name==='invalid-checksum').error,'mixed-case checksum refusal required');
const result=JSON.stringify({oracle:'pinned ethers package files; primitive operations, not venue payload construction',cases,bytesCases,addresses},null,2)+'\n',target=path.join(__dirname,'primitives.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'Ethereum primitive oracle drift');
