// Real WASM device signatures, checked by the independent official Go-WASM crypto.
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {initializeLighterCrypto} from './lighter-crypto.mjs';
import {parseRequestJson} from './request-json.mjs';
const corpus=parseRequestJson(readFileSync(new URL('../venues/lighter/tests/fixtures/modify-signing.json',import.meta.url)));
const words=(values)=>Buffer.concat(values.map(value=>{const b=Buffer.alloc(8);b.writeBigUInt64LE(BigInt(value));return b;})).toString('hex');
function statement(c) {
 const i=c.txInfo;
 return JSON.stringify({kind:'txs',chainId:c.chainId,accountIndex:i.AccountIndex,apiKeyIndex:i.ApiKeyIndex,
  txs:[{txType:17,nonce:i.Nonce,expiredAt:i.ExpiredAt,modify:{MarketIndex:i.MarketIndex,Index:i.Index,BaseAmount:i.BaseAmount,Price:i.Price,TriggerPrice:i.TriggerPrice}}]},(_key,value)=>typeof value==='bigint'?JSON.rawJSON(value.toString()):value);
}
export async function checkLighterModifySigning(WasmEngine) {
 const oracle=await initializeLighterCrypto();
 const publicKey=oracle.schnorrPkFromSk(corpus.secret);
 for(const c of corpus.cases) {
  const text=statement(c);
  const i=c.txInfo;
  const bytes=words([c.chainId,17,i.Nonce,i.ExpiredAt,i.AccountIndex,i.ApiKeyIndex,i.MarketIndex,i.Index,i.BaseAmount,i.Price,i.TriggerPrice]);
  assert.equal(oracle.schnorrVerify(bytes,publicKey,c.signatureHex),true,'official signature');
  assert.equal(oracle.poseidon2HashFp5(bytes).replace(/^0x/,''),c.txHash);
  for(const capsule of [false,true]) {
   const venue=c.chainId===466324?'lighter-robinhood':'lighter';
   const options={applicationId:'42',apiKeyIndex:'3',testnet:c.chainId===300};
   const engine=new WasmEngine(venue,JSON.stringify(capsule?options:{...options,secret:corpus.secret}),capsule?JSON.stringify({primary:{secret:corpus.secret}}):undefined);
   try {
    const signature=engine.hostedWalletSigning(text,'lighter-tx',i.ExpiredAt-600000);
    assert.equal(oracle.schnorrVerify(bytes,publicKey,signature.replace(/^0x/,'')),true,'WASM signature');
    for(let word=0;word<11;word++) {
     const changed=Buffer.from(bytes,'hex');changed[word*8]^=1;
     assert.equal(oracle.schnorrVerify(changed.toString('hex'),publicKey,signature.replace(/^0x/,'')),false,`tampered word${word}`);
    }
    for(const changed of [text.replace('"txType":17','"txType":15'),text.replace('"chainId":'+c.chainId,'"chainId":'+(c.chainId===300?304:300)),text.replace('"accountIndex":42','"accountIndex":43'),text.replace('"apiKeyIndex":3','"apiKeyIndex":4'),text.replace('"MarketIndex":'+i.MarketIndex,'"MarketIndex":255'),text.replace('"modify":{','"modify":{"Type":2,')]) {
     assert.throws(()=>engine.hostedWalletSigning(changed,'lighter-tx',i.ExpiredAt-600000));
    }
    assert.throws(()=>engine.hostedWalletSigning(text,'lighter-tx',i.ExpiredAt));
    assert.throws(()=>engine.hostedWalletSigning(text,'lighter-tx',i.ExpiredAt-630001));
   } finally { engine.free(); }
  }
 }
}
