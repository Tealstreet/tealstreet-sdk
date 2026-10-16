// Independent SNIP-12 revision 0 and 1 oracle: installed Starknet JS SDK.
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const {createHash}=require('node:crypto');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');
verifyPackage('starknet');
const {typedData,ec}=oracleRequire('starknet');
const fixture=path.join(__dirname,'typed-data.json');
const cases=[];
for(const revision of [0,1]) {
 const domainName=revision===0?'StarkNetDomain':'StarknetDomain';
 const scalarType=revision===0?'felt':'shortstring';
 const data={types:{[domainName]:[{name:'name',type:scalarType},{name:'version',type:scalarType},{name:'chainId',type:scalarType},...(revision===1?[{name:'revision',type:'shortstring'}]:[])],Message:[{name:'zPayload',type:'felt'},{name:'alphaNonce',type:'felt'}]},primaryType:'Message',domain:{name:'Fixture',version:'1',chainId:'TEST',...(revision===1?{revision:'1'}:{})},message:{zPayload:'42',alphaNonce:'7'}};
 const raw=JSON.stringify(data);const address='0x1234';const key='0x1';
 const hash=typedData.getMessageHash(JSON.parse(raw),address);const signature=ec.starkCurve.sign(hash,key);
 const changedPayload=raw.replace('"zPayload":"42"','"zPayload":"43"');
 const changedNonce=raw.replace('"alphaNonce":"7"','"alphaNonce":"8"');
 const changedPayloadHash=typedData.getMessageHash(JSON.parse(changedPayload),address);
 const changedNonceHash=typedData.getMessageHash(JSON.parse(changedNonce),address);
 const changedAddressHash=typedData.getMessageHash(JSON.parse(raw),'0x1235');
 assert.notEqual(changedPayloadHash,hash);assert.notEqual(changedNonceHash,hash);assert.notEqual(changedAddressHash,hash);
 assert.equal(ec.starkCurve.verify(signature,changedPayloadHash,ec.starkCurve.getPublicKey(key)),false);
 assert.equal(ec.starkCurve.verify(signature,changedNonceHash,ec.starkCurve.getPublicKey(key)),false);
 assert.equal(ec.starkCurve.verify(signature,hash,ec.starkCurve.getPublicKey('0x2')),false);
 cases.push({revision,raw,address,key,hash,publicKey:ec.starkCurve.getStarkKey(key),r:'0x'+signature.r.toString(16),s:'0x'+signature.s.toString(16),changedPayload,changedNonce,changedPayloadHash,changedNonceHash,changedAddressHash});
}
const result=JSON.stringify({oracle:'installed Starknet JS typedData.getMessageHash and ec.starkCurve.sign; generic fixture domains, SNIP-12 revisions 0 and 1',sdkSha256:createHash('sha256').update(fs.readFileSync(oracleRequire.resolve('starknet'))).digest('hex'),cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(fixture,result,{flag:'wx'});
else assert.equal(result,fs.readFileSync(fixture,'utf8'),'Stark typed-data oracle drift');
console.log('Stark typed-data revision/key/payload/nonce/address controls passed');
