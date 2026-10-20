import test from 'node:test';
import assert from 'node:assert/strict';
import { kucoinspot } from './kucoinspot.mjs';
const endpoint='wss://spot.example.test/endpoint';
const records=['public','private'].map((role)=>({type:'http',url:`https://api.kucoin.com/api/v1/bullet-${role}`,body:JSON.stringify({code:'200000',data:{token:role+'-token',instanceServers:[{endpoint,pingInterval:18000,pingTimeout:10000}]}})}));
const headers={ 'KC-API-KEY':'replay-key','KC-API-KEY-VERSION':'2','KC-API-PASSPHRASE':'placeholder','KC-API-TIMESTAMP':'1700000000123','KC-API-SIGN':'placeholder' };
test('spot request matching preserves caller identity, HF endpoint and raw numeric strings',()=>{
 const ids=new Set(['caller-1']);
 const key=kucoinspot.requestKey('POST','https://api.kucoin.com/api/v1/hf/orders',JSON.stringify({symbol:'BTC-USDT',clientOid:'caller-1',size:'1',price:'90'}),ids);
 assert.notEqual(key,kucoinspot.requestKey('POST','https://api.kucoin.com/api/v1/hf/orders',JSON.stringify({symbol:'BTC-USDT',clientOid:'caller-2',size:'1',price:'90'}),ids));
 assert.notEqual(key,kucoinspot.requestKey('POST','https://api.kucoin.com/api/v1/hf/orders/alter',JSON.stringify({symbol:'BTC-USDT',clientOid:'caller-1',size:'1',price:'90'}),ids));
});
test('public-read allowlist cannot admit private account reads or a different host',()=>{
 assert.equal(kucoinspot.checkRequest('GET','https://api.kucoin.com/api/v2/symbols','',{}),true);
 assert.equal(kucoinspot.checkRequest('GET','https://api.kucoin.com/api/v1/accounts?type=trade','',{}),false);
 assert.equal(kucoinspot.checkRequest('GET','https://other.test/api/v2/symbols','',{}),false);
 assert.equal(kucoinspot.checkRequest('POST','https://api.kucoin.com/api/v1/hf/orders','{}',headers),false);
});
test('spot socket audit requires exact bullet token, role and book topics',()=>{
 const audit=kucoinspot.createSocketAudit('rust',{isPrivate:true,books:['BTC-USDT'],records});
 for(const role of ['public','private']){
  const url=`${endpoint}?token=${role}-token&connectId=test`;
  audit.received(url,JSON.stringify({type:'welcome'}));
  const topics=role==='public'?['/market/ticker:all','/spotMarket/level2Depth50:BTC-USDT']:['/spotMarket/tradeOrdersV2','/account/balance'];
  for(const topic of topics)audit.sent(url,JSON.stringify({type:'subscribe',topic,privateChannel:role==='private',response:true}));
 }
 assert.equal(audit.ready(),true);assert.deepEqual(audit.errors(),[]);
 const invalid=kucoinspot.createSocketAudit('rust',{isPrivate:false,books:[],records});
 invalid.sent(endpoint+'?token=public-token&connectId=test',JSON.stringify({type:'subscribe',topic:'/market/ticker:all',privateChannel:true}));
 assert.match(invalid.errors().join('\n'),/before welcome|channel/);
});
