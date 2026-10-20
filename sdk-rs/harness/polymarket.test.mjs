import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { createRequire } from 'node:module';
import { POLYMARKET_REPLAY,verifyPolymarketRequest,polymarket } from './polymarket.mjs';
import { requestKey,subscriptionKeys } from './requests.mjs';
import { axiosClients } from './venues.mjs';
const require=createRequire(new URL('../../packages/safe-cex/package.json',import.meta.url));
const {ethers}=require('ethers');
const wallet=new ethers.Wallet(POLYMARKET_REPLAY.key), zero=`0x${'00'.repeat(32)}`;
const secret=Buffer.from(POLYMARKET_REPLAY.polymarket.apiSecret,'base64url');
const fields=[['salt','uint256'],['maker','address'],['signer','address'],['tokenId','uint256'],['makerAmount','uint256'],['takerAmount','uint256'],['side','uint8'],['signatureType','uint8'],['timestamp','uint256'],['metadata','bytes32'],['builder','bytes32']].map(([name,type])=>({name,type}));
function request(method,path,body='') {
  const stamp=String(Math.floor(Date.now()/1000));
  return {method,url:`https://clob.polymarket.com${path}`,body,headers:{POLY_ADDRESS:wallet.address,POLY_API_KEY:POLYMARKET_REPLAY.polymarket.apiKey,POLY_PASSPHRASE:POLYMARKET_REPLAY.polymarket.apiPassphrase,POLY_TIMESTAMP:stamp,POLY_SIGNATURE:createHmac('sha256',secret).update(stamp+method+path.split('?')[0]+body).digest('base64').replace(/\+/g,'-').replace(/\//g,'_')}};
}
async function order(tokenId='1',timestamp=String(Date.now())) {
  const data={salt:123,maker:wallet.address,signer:wallet.address,tokenId,makerAmount:'2500000',takerAmount:'5000000',side:0,signatureType:0,timestamp,metadata:zero,builder:zero};
  const signature=await wallet.signTypedData({name:'Polymarket CTF Exchange',version:'2',chainId:137,verifyingContract:tokenId==='3'?'0xe2222d279d744050d28e00520010520000310F59':'0xE111180000d2663C0091e4f400237545B87B996B'},{Order:fields},data);
  return {deferExec:false,postOnly:false,order:{...data,side:'BUY',expiration:'0',signature},owner:POLYMARKET_REPLAY.polymarket.apiKey,orderType:'GTC'};
}
test('Polymarket raw auth checks exact body bytes and excludes only transmitted query params',()=>{
  const r=request('GET','/data/orders?next_cursor=MA%3D%3D');assert.equal(verifyPolymarketRequest(r),true);
  const write=request('DELETE','/orders','["id"]');assert.equal(verifyPolymarketRequest(write),true);assert.equal(verifyPolymarketRequest({...write,body:'["different"]'}),false);
  for(const name of ['POLY_ADDRESS','POLY_API_KEY','POLY_PASSPHRASE','POLY_TIMESTAMP','POLY_SIGNATURE']) assert.equal(verifyPolymarketRequest({...write,headers:{...write.headers,[name]:'wrong'}}),false,name);
});
test('Independent EIP712 verifier checks standard and negative-risk V2 bodies',async()=>{
  for(const token of ['1','3']) {const payload=await order(token);assert.equal(verifyPolymarketRequest(request('POST','/order',JSON.stringify(payload))),true);}
  const payload=await order();
  for(const key of ['makerAmount','takerAmount','tokenId','timestamp','side','signatureType','metadata','builder']) {
    const changed=structuredClone(payload);changed.order[key]=key==='side'?'SELL':key==='signatureType'?1:key==='metadata'||key==='builder'?`0x${'11'.repeat(32)}`:'99';
    assert.equal(verifyPolymarketRequest(request('POST','/order',JSON.stringify(changed))),false,key);
  }
  assert.equal(verifyPolymarketRequest(request('POST','/order','not-json')),false);
});
test('Economic request identity retains amounts/side/token but masks independently verified entropy',async()=>{
  const first=await order(),second=await order();second.order.salt=456;second.order.timestamp='123';second.order.signature='random';
  const key=o=>requestKey('POST','https://clob.polymarket.com/order',JSON.stringify(o),new Set(),polymarket);
  assert.equal(key(first),key(second));second.order.makerAmount='99';assert.notEqual(key(first),key(second));
});
test('Subscription batches preserve every asset and feature flag, with typed POST reads',()=>{
  const url='wss://ws-subscriptions-clob.polymarket.com/ws/market';
  const a=subscriptionKeys(url,{type:'market',assets_ids:['1','2'],custom_feature_enabled:true},new Set(),polymarket).sort();
  const b=['2','1'].flatMap(id=>subscriptionKeys(url,{operation:'subscribe',assets_ids:[id],custom_feature_enabled:true},new Set(),polymarket)).sort();assert.deepEqual(a,b);
  assert.notDeepEqual(a,subscriptionKeys(url,{type:'market',assets_ids:['1','2'],custom_feature_enabled:false},new Set(),polymarket));
  assert.equal(polymarket.isWrite('POST','https://clob.polymarket.com/midpoints'),false);assert.equal(polymarket.isWrite('POST','https://clob.polymarket.com/order'),true);
});
test('Generic client discovery captures nested CLOB, Gamma and Data transports exactly once',()=>{
  const client=()=>({defaults:{},interceptors:{}});const a=client(),b=client(),c=client();assert.deepEqual(new Set(axiosClients({api:{xhr:a,gammaXhr:b,dataXhr:c},xhr:a})),new Set([a,b,c]));
});

test('Freshly signed but stale or future order timestamps cannot hide behind normalization',async()=>{
  for(const stamp of ['0',String(Date.now()-120000),String(Date.now()+120000)]) {
    const body=JSON.stringify(await order('1',stamp));assert.equal(verifyPolymarketRequest(request('POST','/order',body)),false);
  }
});
test('DELETE order identity never ignores an unexpected nested order field',()=>{
  const url='https://clob.polymarket.com/order';const original={orderID:'original'};const extra={...original,order:{salt:999,timestamp:'0',signature:'bad'}};
  assert.notEqual(requestKey('DELETE',url,original,new Set(),polymarket),requestKey('DELETE',url,extra,new Set(),polymarket));
});

test('Startup requires every book asset and the independently configured private credentials',()=>{
  const records=[{type:'config',private:true,options:POLYMARKET_REPLAY},{type:'http',status:200,url:'https://gamma-api.polymarket.com/markets/keyset',body:JSON.stringify({markets:[{slug:'fixture',clobTokenIds:'["1","2"]',outcomes:'["Yes","No"]'}]})}];
  const keys=polymarket.startupSubscriptions(records,['fixture-Yes','fixture-No']);
  assert.equal(keys.length,3);assert.equal(keys.filter(key=>key.includes('/ws/market')).length,2);assert.equal(keys.filter(key=>key.includes('/ws/user')&&key.includes(POLYMARKET_REPLAY.polymarket.apiKey)).length,1);
  assert.throws(()=>polymarket.startupSubscriptions(records,['unknown']),/Missing taped/);
});
test('Valid L1 signatures with a stale timestamp are rejected before normalization',async()=>{
  const timestamp='0';const signature=await wallet.signTypedData({name:'ClobAuthDomain',version:'1',chainId:137},{ClobAuth:[{name:'address',type:'address'},{name:'timestamp',type:'string'},{name:'nonce',type:'uint256'},{name:'message',type:'string'}]},{address:wallet.address,timestamp,nonce:'0',message:'This message attests that I control the given wallet'});
  assert.equal(verifyPolymarketRequest({method:'GET',url:'https://clob.polymarket.com/auth/derive-api-key',headers:{POLY_ADDRESS:wallet.address,POLY_TIMESTAMP:timestamp,POLY_NONCE:'0',POLY_SIGNATURE:signature}}),false);
});
