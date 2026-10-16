// Independent installed Starknet JS SDK oracle; never imports Rust output.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash} = require('node:crypto');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');
verifyPackage('starknet');
const {ec} = oracleRequire('starknet');
const curve = ec.starkCurve;
const fixture = path.join(__dirname,'sdk.json');
const n = curve.CURVE.n;
const mod = a => ((a % n) + n) % n;
const inverse = a => { let b=n, x=1n,y=0n; while(b){ const q=a/b; [a,b]=[b,a-q*b]; [x,y]=[y,x-q*y]; } return mod(x); };
const hex = x => '0x'+x.toString(16);
const cases=[];
for(const [i,messageHash] of ['0x1234','0x2102820b232636d200cb21f1d330f20d096cae09d1bf3edb1cc333ddee11318','0x1'].entries()) {
  const key=hex(BigInt(i+1));
  const publicKey=curve.getStarkKey(key);
  const signature=curve.sign(messageHash,key);
  // Recover the nonce from independently produced SDK r/s, not a Rust RFC6979 expression.
  const nonce=mod((BigInt(messageHash)+signature.r*BigInt(key))*inverse(signature.s));
  const changed=curve.sign(messageHash,key,{extraEntropy:new Uint8Array(32).fill(1)});
  const changedNonce=mod((BigInt(messageHash)+changed.r*BigInt(key))*inverse(changed.s));
  const changedPayload=hex(BigInt(messageHash)+1n);
  const changedKey=hex(BigInt(i+11));
  const fullPublicKey=curve.getPublicKey(key);
  assert.equal(curve.verify(signature,messageHash,fullPublicKey),true);
  assert.equal(curve.verify(signature,changedPayload,fullPublicKey),false);
  assert.equal(curve.verify(signature,messageHash,curve.getPublicKey(changedKey)),false);
  assert.notEqual(changedNonce,nonce);
  assert.notEqual(changed.toCompactHex(),signature.toCompactHex());
  cases.push({key,publicKey,messageHash,nonce:hex(nonce),r:hex(signature.r),s:hex(signature.s),changedKey,changedPayload,changedNonce:hex(changedNonce),changedR:hex(changed.r),changedS:hex(changed.s)});
}
const entry=oracleRequire.resolve('starknet');
const result=JSON.stringify({oracle:'installed starknet ec.starkCurve.sign; nonce recovered from SDK signature, extraEntropy changes nonce', sdkSha256:createHash('sha256').update(fs.readFileSync(entry)).digest('hex'),cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(fixture,result,{flag:'wx'});
else assert.equal(result,fs.readFileSync(fixture,'utf8'),'Starknet SDK oracle drift');
console.log('Starknet SDK oracle key/payload/nonce controls passed');
