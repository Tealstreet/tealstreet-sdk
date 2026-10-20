import test from 'node:test';
import assert from 'node:assert/strict';
import { kucoinspot } from './kucoinspot.mjs';
import { createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { Tape } from './tape.mjs';
import { Responses } from './responses.mjs';
import { requestKey, subscriptionKeys } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';
import { venueOf } from './venues.mjs';
const endpoint='wss://spot.example.test/endpoint';
const records=['public','private'].map((role)=>({type:'http',method:'POST',status:200,url:`https://api.kucoin.com/api/v1/bullet-${role}`,body:JSON.stringify({code:'200000',data:{token:role+'-token',instanceServers:[{endpoint,pingInterval:18000,pingTimeout:10000}]}})}));
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

const productionTape=(httpRecords)=>{
 const source=readFileSync(new URL('./replay.mjs',import.meta.url),'utf8');
 const start=source.indexOf('const VOLATILE ='),cs=source.indexOf('class Tape extends SharedTape {'),end=source.indexOf('\nconst settle =',cs);
 assert(start>=0 && cs>start && end>cs);
 const ProductionTape=new Function('SharedTape','values','venue','records','bookSymbols','callerIds','keyOf','Responses','sharedUrl','parseRequestJson','isRead','isPrivate',
  `${source.slice(start,cs)}\nreturn (${source.slice(cs,end)});`)(Tape,{venue:'kucoinspot'},venueOf('kucoinspot'),httpRecords,[],new Set(),requestKey,Responses,undefined,parseRequestJson,(m)=>m==='GET',true);
 return new ProductionTape('ts');
};
const sign=(method,url,body='')=>{
 const parsed=new URL(url),timestamp=String(Date.now());
 const path=parsed.pathname+(parsed.search?'?'+[...parsed.searchParams].map(([k,v])=>`${k}=${v}`).join('&'):'');
 const hmac=(value)=>createHmac('sha256','replay-secret').update(value).digest('base64');
 return {'KC-API-KEY':'replay-key','KC-API-KEY-VERSION':'2','KC-API-PASSPHRASE':hmac('replay-passphrase'),
  'KC-API-TIMESTAMP':timestamp,'KC-API-SIGN':hmac(timestamp+method+path+body)};
};
test('production Spot HTTP keeps transport-looking queries and scopes bounded generated IDs to actual placements',()=>{
 const route='https://api.kucoin.com/api/v1/hf/orders';
 const order={clientOid:'ABCDEFGHIJKLMNOPQRSTU',symbol:'BTC-USDT',type:'limit',side:'buy',size:'1',price:'90'};
 const record={type:'http',method:'POST',url:route,requestBody:JSON.stringify(order),status:200,body:'{"code":"200000","data":{"orderId":"1"}}'};
 const tape=productionTape([record]);
 for(const key of ['connectId','timestamp','recvWindow','signature','requestId','clientOrderId','unknownEconomicField']){
  const changedUrl=route+'?'+new URLSearchParams({[key]:'extra'}),raw=JSON.stringify(order);
  assert.equal(tape.respond('POST',changedUrl,raw,sign('POST',changedUrl,raw)).status,599,key);
  assert.equal(tape.missingWrites,1);
 }
 for(const clientOid of ['', 'a'.repeat(22), 'bad-id', 'bad_id', 'bad\n', 'bad\u2028', 123, null]){
  const raw=JSON.stringify({...order,clientOid});
  assert.equal(tape.respond('POST',route,raw,sign('POST',route,raw)).status,599,JSON.stringify(clientOid));
  assert.equal(tape.missingWrites,1);
 }
 for(const changed of [{size:1},{price:'91'},{symbol:'ETH-USDT'},{unknownEconomicField:1},
  {clientOrderId:'undeclared'},{newClientOrderId:'undeclared'},{clientAlgoId:'undeclared'}]){
  const raw=JSON.stringify({...order,...changed});
  assert.equal(tape.respond('POST',route,raw,sign('POST',route,raw)).status,599);
  assert.equal(tape.missingWrites,1);
 }
 const uuidSource=readFileSync(new URL('../../packages/safe-cex/src/utils/uuid.ts',import.meta.url),'utf8');
 const start=uuidSource.indexOf('export const uuid');
 const actualGenerated=new Function('nanoid',uuidSource.slice(start).replace('export const uuid','const uuid')+'; return uuid();')(()=> 'aB-_cD0123456789abcdef');
 assert.match(actualGenerated,/^[A-Za-z0-9]{1,21}$/);
 const valid=JSON.stringify({...order,clientOid:actualGenerated});
 const invalidHeaders={...sign('POST',route,valid),'KC-API-SIGN':'invalid'};
 assert.equal(tape.respond('POST',route,valid,invalidHeaders).status,599);
 assert.equal(tape.missingWrites,1);
 assert.equal(tape.respond('POST',route,valid,sign('POST',route,valid)).status,200);
 assert.equal(tape.missingWrites,0);assert.equal(tape.respond('POST',route,valid,sign('POST',route,valid)).status,599);
 for(const [method,path] of [['POST','/api/v1/hf/orders/alter'],['DELETE','/api/v1/hf/orders']]){
  const url='https://api.kucoin.com'+path;
  const exact=productionTape([{...record,method,url}]);
  assert.equal(exact.respond(method,url,valid,sign(method,url,valid)).status,599);
  assert.equal(exact.missingWrites,1);
  assert.equal(exact.respond(method,url,record.requestBody,sign(method,url,record.requestBody)).status,200);
 }
 const caller=productionTape([record,{type:'command',op:'placeOrders',orders:[{clientOrderId:order.clientOid}]}]);
 assert.equal(caller.respond('POST',route,valid,sign('POST',route,valid)).status,599);
 assert.equal(caller.missingWrites,1);
 assert.equal(caller.respond('POST',route,record.requestBody,sign('POST',route,record.requestBody)).status,200);
});

test('Spot socket IDs vary only in socket identity, retaining tokens and scoped topics',()=>{
 const options={...venueOf('kucoinspot'),fullSocketIdentity:true};
 const url=endpoint+'?token=public-token&connectId=first';
 const keys=(url)=>subscriptionKeys(url,{type:'subscribe',topic:'/market/ticker:all',privateChannel:false},new Set(),options);
 assert.deepEqual(keys(url),keys(url.replace('connectId=first','connectId=second')));
 assert.notDeepEqual(keys(url),keys(url.replace('public-token','private-token')));
 assert.notDeepEqual(keys(url),keys(url+'&readonly=true'));
});

test('actual Spot sockets cannot borrow welcomes topics or readiness from same-URL peers and replacements',()=>{
 const url=endpoint+'?token=public-token&connectId=shared',topic='/market/ticker:all';
 for(const parallel of [false,true]){
  const tape=new Tape(records,venueOf('kucoinspot'),false,100,'rust');
  const first=tape.connect(url,()=>{},()=>{});
  first.onMessage('{"type":"welcome"}');
  const subscribe=JSON.stringify({type:'subscribe',topic,privateChannel:false});
  tape.send(first,subscribe);assert.equal(tape.audit.ready(),true);
  if(!parallel)tape.close(first,1000,true);
  const next=tape.connect(url,()=>{},()=>{});
  assert.equal(tape.audit.ready(),parallel,'closed socket coverage cannot authorize a replacement');
  tape.send(next,'{"type":"ping","id":"new"}');
  next.onMessage(JSON.stringify({type:'message',topic}));
  assert(tape.audit.errors().some((error)=>error.includes('before welcome')));
  assert(tape.audit.errors().some((error)=>error.includes('unsubscribed')));
  next.onMessage('{"type":"welcome"}');tape.send(next,subscribe);
  const before=tape.audit.errors().length;
  if(parallel){
   tape.send(next,JSON.stringify({type:'unsubscribe',topic,privateChannel:false}));
   first.onMessage(JSON.stringify({type:'message',topic}));
   assert.equal(tape.audit.errors().length,before,'peer unsubscribe cannot revoke first socket topics');
  }
 }
});

for(const [name,change] of [
 ['duplicate-case API key',(url,headers)=>({url,headers:{'kc-api-key':'wrong-key',...headers}})],
 ['URL userinfo',(url,headers)=>({url:url.replace('https://','https://not-approved@'),headers})],
 ['URL fragment',(url,headers)=>({url:url+'#unrecognized',headers})],
 ['empty URL fragment',(url,headers)=>({url:url+'#',headers})],
])test(`production Spot rejects ${name} before consuming an otherwise valid ACK`,()=>{
 const url='https://api.kucoin.com/api/v1/hf/orders';
 const body=JSON.stringify({clientOid:'ABCDEFGHIJKLMNOPQRSTU',symbol:'BTC-USDT',type:'limit',side:'buy',size:'1',price:'90'});
 const record={type:'http',method:'POST',url,requestBody:body,status:200,body:'{"code":"200000","data":{"orderId":"1"}}'};
 const tape=productionTape([record]);
 const changed=change(url,sign('POST',url,body));
 assert.equal(tape.respond('POST',changed.url,body,changed.headers).status,599);
 assert.equal(tape.unserved,1,'caught request failures must remain fatal to the gate');
 assert.equal(tape.missingWrites,1);
 assert.equal(tape.respond('POST',url,body,sign('POST',url,body)).status,200);
 assert.equal(tape.missingWrites,0);
 assert.equal(tape.respond('POST',url,body,sign('POST',url,body)).status,599);
});

for(const field of ['timestamp','recvWindow','signature','requestId']){
 test(`production Spot retains unsupported signed body ${field} with every value shape`,()=>{
  const url='https://api.kucoin.com/api/v1/hf/orders';
  const order={clientOid:'ABCDEFGHIJKLMNOPQRSTU',symbol:'BTC-USDT',type:'limit',side:'buy',size:'1',price:'90'};
  const body=JSON.stringify(order);
  const tape=productionTape([{type:'http',method:'POST',url,requestBody:body,status:200,body:'{"code":"200000","data":{"orderId":"1"}}'}]);
  for(const value of ['unknown-business-field',123,null,{nested:{signature:'business',clientOid:'nested-identity'}},[1,'2',false]]){
   const changed=JSON.stringify({...order,[field]:value});
   assert.equal(kucoinspot.checkRequest('POST',url,changed,sign('POST',url,changed)),true,'raw signature is valid');
   assert.equal(tape.respond('POST',url,changed,sign('POST',url,changed)).status,599,JSON.stringify(value));
   assert.equal(tape.missingWrites,1);
  }
  const nested=JSON.stringify({...order,extra:{[field]:{nested:'business'}}});
  assert.equal(tape.respond('POST',url,nested,sign('POST',url,nested)).status,599);
  assert.equal(tape.missingWrites,1);
  assert.equal(tape.unserved,6,'every unsupported body field remains a sticky unmatched request');
  assert.equal(tape.respond('POST',url,body,sign('POST',url,body)).status,200);
  assert.equal(tape.missingWrites,0);
  assert.equal(tape.respond('POST',url,body,sign('POST',url,body)).status,599);
 });
}
