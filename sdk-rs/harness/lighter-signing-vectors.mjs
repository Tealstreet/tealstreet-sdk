// Offline original TS SDK/Go-WASM capture for current int16 market indexes.
import {writeFileSync,readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {TransactionClient} from '../../packages/lighter-sdk/src/client/transaction-client.ts';
import {initializeLighterCrypto} from './lighter-crypto.mjs';
const oracle=await initializeLighterCrypto('ts');
const now=1700000000000, secret='01'+'00'.repeat(39);
Date.now=()=>now;
const cases=[];
const fields=(values)=>Buffer.concat(values.map(value=>{const b=Buffer.alloc(8);b.writeBigUInt64LE(BigInt(value));return b;})).toString('hex');
for(const marketId of [4098,32767]){
 const client=()=>new TransactionClient({httpClient:{getNextNonce:async()=>7n},crypto:oracle,chainId:304,privateKey:secret,accountIndex:42n,apiKeyIndex:3});
 for(const [name,tx] of [
  ['create',await client().getCreateOrderTransaction({orderId:123n,marketId,isBuy:true,price:10000n,amount:250n,orderType:'limit',timeInForce:'GoodTillCancel'})],
  ['cancel',await client().getCancelOrderTransaction({orderId:123n,marketId})],
 ])cases.push({name,marketId,payload:tx.data,signature:tx.signature,source:'original TS TransactionClient and checked-in Go-WASM'});
 // TS SDK's leverage validator still caps IDs at254. The signed protocol field
 // is int16 in official lighter-go; use that documented field order as oracle.
 const payload=fields([304,20,7,now+600000,42,3,marketId,2000,0]);
 cases.push({name:'leverage',marketId,payload,signature:oracle.schnorrSign(payload,secret),source:'official lighter-go L2UpdateLeverageTxInfo.Hash field order and checked-in Go-WASM'});
}
const wasm=readFileSync(new URL('../../packages/poseidon-crypto/src/wasm/poseidon_crypto.wasm',import.meta.url));
const output={purpose:'Public offline signing vectors, never a venue credential',officialMarketBounds:'https://pkg.go.dev/github.com/elliottech/lighter-go/types/txtypes#pkg-constants',wasmSha256:createHash('sha256').update(wasm).digest('hex'),now,secret,publicKey:oracle.schnorrPkFromSk(secret),cases};
writeFileSync(new URL('../venues/lighter/tests/fixtures/high-market-signing.json',import.meta.url),JSON.stringify(output,null,2)+'\n');
console.log(JSON.stringify({vectors:cases.length,markets:[4098,32767]}));
process.exit(0);
