import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKrakenRequestAudit,krakenSignature,validateKrakenSocketMessage as sent,validateKrakenSocketFrame as received,krakenRequestBody} from './kraken-protocol.mjs';
import {requestKey,subscriptionKeys} from './requests.mjs';
const now=1700000000000;
function request(method,path,bytes,nonce='1700000000000000'){
 const url='https://futures.kraken.com'+path+(method==='GET'&&bytes?'?'+bytes:'');
 return {method,url,body:method==='GET'?undefined:bytes,now,headers:{APIKey:'replay-key',Nonce:nonce,Authent:krakenSignature(bytes+nonce+path.replace(/^\/derivatives/,''))}};
}
test('REST oracle validates encoded sent bytes and exact path for both API families',()=>{
 for(const [method,path,bytes]of [['POST','/derivatives/api/v3/sendorder','broker=AA73+N84G+7YOF+J26Y&cliOrdId=ts_test&symbol=PF_XBTUSD'],['GET','/api/history/v3/account-log','count=500&since=1700000000000']]){
  assert.doesNotThrow(()=>createKrakenRequestAudit()(request(method,path,bytes)));
  assert.throws(()=>createKrakenRequestAudit()({...request(method,path,bytes),body:bytes+'x',url:method==='GET'?'https://futures.kraken.com'+path+'?'+bytes+'x':'https://futures.kraken.com'+path}));
 }
});
test('REST oracle rejects replay nonces, stale timestamps and credential leakage',()=>{
 const audit=createKrakenRequestAudit(),r=request('POST','/derivatives/api/v3/sendorder','cliOrdId=ts_test&size=1');audit(r);assert.throws(()=>audit(r),/nonce/);assert.throws(()=>createKrakenRequestAudit()({...r,now:now+60001}),/nonce/);
 assert.throws(()=>createKrakenRequestAudit()({...r,method:'GET',url:'https://futures.kraken.com/derivatives/api/v3/tickers'}),/leaked/);
});
test('challenge subscriptions require current socket challenge and exact signature',()=>{
 const s={};sent(s,JSON.stringify({event:'challenge',api_key:'replay-key'}));received(s,JSON.stringify({event:'challenge',message:'test-challenge'}));
 const m={event:'subscribe',feed:'fills',api_key:'replay-key',original_challenge:'test-challenge',signed_challenge:krakenSignature('test-challenge')};sent(s,JSON.stringify(m));received(s,JSON.stringify({feed:'fills',fills:[]}));
 assert.throws(()=>sent({},JSON.stringify(m)),/current/);assert.throws(()=>sent(s,JSON.stringify({...m,signed_challenge:'bad'})),/current/);assert.throws(()=>received({},JSON.stringify({feed:'fills',fills:[]})),/before/);
});
test('form request normalization keeps batch economic fields and only minted IDs vary',()=>{
 const a=krakenRequestBody({json:JSON.stringify({batchOrder:[{order:'cancel',order_id:'001'}]})});assert.deepEqual(a,{batchOrder:[{order:'cancel',order_id:'001'}]});
 const opt={requestBody:krakenRequestBody};assert.equal(requestKey('POST','https://futures.kraken.com/derivatives/api/v3/sendorder','cliOrdId=ts_a&size=1',new Set(),opt),requestKey('POST','https://futures.kraken.com/derivatives/api/v3/sendorder','cliOrdId=ts_b&size=1',new Set(),opt));
 assert.equal(subscriptionKeys('wss://futures.kraken.com/ws/v1',{event:'subscribe',feed:'ticker',product_ids:['PF_XBTUSD','PF_ETHUSD']},new Set(),{productSubscriptions:true}).length,2);
});

test('minted IDs must be unique even when validly signed',()=>{
 const a=createKrakenRequestAudit();a(request('POST','/derivatives/api/v3/sendorder','cliOrdId=ts_test&size=1'));
 assert.throws(()=>a(request('POST','/derivatives/api/v3/sendorder','cliOrdId=ts_test&size=1','1700000000000001')),/reused/);
 assert.throws(()=>createKrakenRequestAudit()(request('POST','/derivatives/api/v3/sendorder','cliOrdId=caller&size=1')),/invalid/);
});
test('a new challenge request invalidates all prior challenge proof and feeds',()=>{
 const s={};const req=JSON.stringify({event:'challenge',api_key:'replay-key'});sent(s,req);received(s,JSON.stringify({event:'challenge',message:'A'}));
 const sub=JSON.stringify({event:'subscribe',feed:'fills',api_key:'replay-key',original_challenge:'A',signed_challenge:krakenSignature('A')});sent(s,sub);sent(s,req);
 assert.throws(()=>sent(s,sub),/current/);assert.throws(()=>received(s,JSON.stringify({feed:'fills',fills:[]})),/before/);
});
test('signed duplicate economic form fields cannot normalize to a valid request',()=>{
 assert.throws(()=>createKrakenRequestAudit()(request('POST','/derivatives/api/v3/sendorder','cliOrdId=ts_test&size=100&size=1')),/duplicate/);
});
test('batch JSON cannot hide a signed target behind duplicate properties',()=>{
 const bytes='json='+encodeURIComponent('{"batchOrder":[{"order":"cancel","order_id":"wrong","order_id":"right"}]}');
 assert.throws(()=>createKrakenRequestAudit()(request('POST','/derivatives/api/v3/batchorder',bytes)),/duplicate JSON/);
});
test('duplicate authentication headers and plaintext upstream URLs fail closed',()=>{
 const r=request('POST','/derivatives/api/v3/sendorder','cliOrdId=ts_test&size=1');
 assert.throws(()=>createKrakenRequestAudit()({...r,headers:[...Object.entries(r.headers),['APIKey','other']]}),/duplicate/);
 assert.throws(()=>createKrakenRequestAudit()({...r,url:r.url.replace('https:','http:')}),/upstream/);
});

test('transport-swallowed socket oracle errors stay fatal at replay completion',async()=>{
 const {Tape}=await import('./tape.mjs');
 const sentTape=new Tape([],{validateSocketMessage(){throw Error('bad-proof')}},true);
 assert.throws(()=>sentTape.send({url:'wss://futures.kraken.com/ws/v1'},'{}'),/bad-proof/);
 assert.throws(()=>sentTape.assertWritesConsumed(),/invalid replay signature/);
 const receivedTape=new Tape([],{validateSocketFrame(){throw Error('bad-frame')}},true);
 assert.throws(()=>receivedTape.deliver({url:'wss://futures.kraken.com/ws/v1',onMessage(){}},'{}'),/bad-frame/);
 assert.throws(()=>receivedTape.assertWritesConsumed(),/invalid replay signature/);
});

test('TS redundant page normalization is exact, command-owned and cannot hide full-page queries',async()=>{
 const {fixKrakenOutput}=await import('./known-ts-fixes.mjs');
 const path='https://futures.kraken.com/derivatives/api/v3/fills';
 const first={type:'http',method:'GET',status:200,url:path,afterCommand:1,body:JSON.stringify({fills:[{fillTime:'2023-11-14T22:13:20.100Z'}]})};
 const cursor=new Date(1700000000001).toISOString();
 const empty={...first,url:path+'?lastFillTime='+encodeURIComponent(cursor),body:'{"fills":[]}'};
 const other={...empty,url:path+'?lastFillTime='+encodeURIComponent(new Date(1700000001001).toISOString())};
 const key=r=>requestKey('GET',r.url);
 const output=()=>({markets:[],books:{},requests:[key(empty),key(other)],subscriptions:[]});
 const ts=output();fixKrakenOutput(ts,[first,empty,other]);assert.deepEqual(ts.requests,[key(other)]);
 const full={...first,afterCommand:2,body:JSON.stringify({fills:Array.from({length:100},()=>({fillTime:'2023-11-14T22:13:20.100Z'}))})};
 const legitimate={...empty,afterCommand:2};
 const mixed=output();fixKrakenOutput(mixed,[first,empty,other,full,legitimate]);assert.deepEqual(mixed.requests,[key(empty),key(other)]);
 const unowned=output();fixKrakenOutput(unowned,[{...first,afterCommand:undefined},{...empty,afterCommand:undefined},other]);assert.deepEqual(unowned.requests,[key(empty),key(other)]);
});

test('full ticker correction derives live statistics and zero funding only from native payloads',async()=>{
 const {fixKrakenOutput}=await import('./known-ts-fixes.mjs');
 const output={markets:[],books:{},requests:[],subscriptions:[],tickers:[{symbol:'PF_XBTUSD',fundingRate:0.5,percentage:-1,volume:2,quoteVolume:200,openInterest:4,mark:99}]};
 const records=[
  {feed:'ticker',product_id:'PF_XBTUSD',last:110,open:100,change:9.3,relative_funding_rate:0.0002,volume:12,volumeQuote:1320,openInterest:7},
  {feed:'ticker',product_id:'PF_XBTUSD',last:120,open:100,volume:0,volumeQuote:0,openInterest:0},
  {event:'subscribed',feed:'ticker',product_id:'PF_XBTUSD',relative_funding_rate:0.5},
 ].map(message=>({type:'ws',text:JSON.stringify(message)}));
 fixKrakenOutput(output,records);
 assert.deepEqual(output.tickers,[{symbol:'PF_XBTUSD',fundingRate:0,percentage:20,volume:0,quoteVolume:0,openInterest:0,mark:99}]);
});
