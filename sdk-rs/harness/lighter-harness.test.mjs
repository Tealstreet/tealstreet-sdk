import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {initializeLighterCrypto,createLighterVerifier,decodeLighterTransactions,lighterRequestBody} from './lighter-crypto.mjs';
import {requestKey,subscriptionKeys} from './requests.mjs';
import {Tape} from './tape.mjs';
import {fixLighterFills,fixLighterCommands} from './known-ts-fixes.mjs';
import {venueOf} from './venues.mjs';
const corpus=JSON.parse(readFileSync(new URL('../crypto/tests/fixtures/lighter/venue-signing-preimages.json',import.meta.url),'utf8'));
const options={secret:corpus.secret,applicationId:'42',apiKeyIndex:'3'};
const oracle=await initializeLighterCrypto();
const sample=(name)=>corpus.cases.find(c=>c.name==='lighter/livenet/'+name);
const form=(name='create-complete')=>sample(name).wire[0].body;
const url='https://mainnet.zklighter.elliot.ai/api/v1/sendTx';
const now=corpus.fixedNow;
const rewrite=(body,edit)=>{const f=new URLSearchParams(body);const i=JSON.parse(f.get('tx_info'));edit(i);f.set('tx_info',JSON.stringify(i));return f.toString();};
const verifier=()=>createLighterVerifier(options,oracle);
test('independent real Go-WASM verifies supported TS transaction vectors and padded ASCII auth',()=>{
 const v=verifier();
 for(const name of ['create-complete','create-missing-optionals','create-market','create-conditional','cancel-large-index','leverage'])assert.equal(v.request({method:'POST',url,body:form(name),now}),true,name);
 const token=sample('auth').result;
 assert.equal(v.auth(token,now),true);
 assert.equal(v.auth(token.replace(':42:',':43:'),now),false);
 assert.equal(v.auth(token,now+3601_000),false);
 assert.equal(v.auth(token.replace(/.$/,'0'),now),false);
});
test('economic transaction mutations cannot survive valid signature normalization',()=>{
 const v=verifier();
 for(const field of ['Price','BaseAmount','MarketIndex','Type','TimeInForce','IsAsk','ReduceOnly','TriggerPrice','Nonce','AccountIndex','ApiKeyIndex'])assert.equal(v.request({method:'POST',url,body:rewrite(form(),i=>{i[field]+=1;}),now}),false,field);
 assert.equal(v.request({method:'POST',url,body:rewrite(form(),i=>{i.L2TxAttributes[2]+=1;}),now}),false);
 assert.equal(v.request({method:'POST',url,body:rewrite(form(),i=>{i.Sig=Buffer.alloc(80).toString('base64');}),now}),false);
 assert.equal(v.request({method:'POST',url,body:form(),now:now+120_000}),false);
});
test('only matched successful acknowledgements consume generated order identity',()=>{
 const v=verifier(),body=form();
 assert.equal(v.request({method:'POST',url,body,now}),true);
 v.matched({url,body:'{"code":21104}'},{body});
 assert.equal(v.request({method:'POST',url,body,now}),true);
 v.matched({url,body:'{"code":200,"tx_hash":"ack"}'},{body});
 assert.equal(v.request({method:'POST',url,body,now}),false);
 assert.throws(()=>v.matched({url,body:'{"code":200,"tx_hash":"ack"}'},{body}),/reused/);
 assert.deepEqual(v.commandResult({ok:['123']},{op:'placeOrders'}),{ok:['<generated>']});
 assert.throws(()=>v.commandResult({ok:['999']},{op:'placeOrders'}),/this command/);
});
test('private REST auth and authenticated subscription require real valid tokens',()=>{
 const v=verifier(),token=sample('auth').result;
 const endpoint='https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders?account_index=42&market_id=255';
 assert.equal(v.request({method:'GET',url:endpoint,now}),false);
 assert.equal(v.request({method:'GET',url:endpoint+'&auth='+token,now}),true);
 assert.equal(v.subscribe({type:'subscribe',channel:'account_all_orders/42',auth:token},now),true);
 assert.equal(v.subscribe({type:'subscribe',channel:'account_all_orders/42',auth:token+'1'},now),false);
});
test('form identity ignores checked entropy and clocks while retaining amounts, fees and nonce',()=>{
 const venue={requestBody:lighterRequestBody};
 const key=(body)=>requestKey('POST',url,body,new Set(),venue);
 assert.equal(key(form()),key(rewrite(form(),i=>{i.Sig='random';i.ExpiredAt+=1;i.OrderExpiry+=1;i.ClientOrderIndex=999;})));
 for(const field of ['Price','BaseAmount','Nonce','MarketIndex'])assert.notEqual(key(form()),key(rewrite(form(),i=>{i[field]+=1;})),field);
 assert.notEqual(key(form()),key(rewrite(form(),i=>{i.L2TxAttributes[2]+=1;})));
 const cancel=form('cancel-large-index');
 assert.notEqual(key(cancel),key(cancel.replace('9007199254740993','9007199254740994')));
 assert.equal(decodeLighterTransactions(cancel)[0].info.Index,9007199254740993n);
 assert.deepEqual(subscriptionKeys('wss://example.test','{"type":"ping","timestamp":123}'),[]);
});
test('strict actual replay transport persists failed independent signature checks',()=>{
 const v=verifier();
 const venue={requestBody:lighterRequestBody,isWrite:()=>true,verifyRequest:r=>v.request({...r,now}),validateMatchedRequest:(record,r)=>v.matched(record,r)};
 const record={type:'http',method:'POST',url,requestBody:form(),status:200,body:'{"code":200}'};
 const tape=new Tape([record],venue,true);
 assert.throws(()=>tape.respond('POST',url,rewrite(form(),i=>{i.Price+=1;})),/invalid replay signature/);
 assert.throws(()=>tape.assertWritesConsumed(),/invalid replay signature/);
 assert.equal(tape.unusedWrites(),1);
});
test('fill oracle uses recorded execution IDs and remains independent of incorrect Rust quantities',()=>{
 const account={fills:[{id:'7',symbol:'ETH',amount:0.5},{id:'7',symbol:'ETH',amount:0.5}]};
 const records=[{type:'config',options:{applicationId:'42'}},{type:'ws',text:JSON.stringify({type:'update/account_all',channel:'account_all:42',data:{fills:[{fillId:'one',orderId:'7',symbol:'ETH',side:'BUY',price:'100',size:'0.5'}]}})}];
 fixLighterFills(account,records);
 assert.deepEqual(account.fills,[{id:'7',symbol:'ETH',side:'buy',price:100,amount:0.5}]);
 assert.notDeepEqual(account.fills,[{id:'7',symbol:'ETH',side:'buy',price:100,amount:1}]);
 const unchanged={fills:[{id:'untouched'}]};fixLighterFills(unchanged,[]);assert.deepEqual(unchanged.fills,[{id:'untouched'}]);
});
test('malformed acknowledgements cannot validate generated command result IDs',()=>{
 for(const reply of [{code:200},{code:200,tx_hash:''},{code:200,tx_hash:[]},{code:200,tx_hash:['one','two']}]){
  const v=verifier(),body=form();v.matched({url,body:JSON.stringify(reply)},{body});
  assert.throws(()=>v.commandResult({ok:['123']},{op:'placeOrders'}),/this command/);
 }
});
test('batch identity reuse is rejected before response matching or consumption',()=>{
 const single=new URLSearchParams(form()).get('tx_info');
 const body=new URLSearchParams({tx_types:'[14,14]',tx_infos:JSON.stringify([single,single])}).toString();
 assert.equal(verifier().request({method:'POST',url:url+'Batch',body,now}),false);
});
test('net-only mode oracle rewrites only the exact inherited error for hedged false',()=>{
 const records=[{type:'command',op:'setPositionMode',hedged:false},{type:'command',op:'setPositionMode',hedged:true},{type:'command',op:'setPositionMode',hedged:false}];
 const results=[{error:'Not implemented'},{error:'Not implemented'},{error:'unexpected'}];fixLighterCommands(results,records);
 assert.deepEqual(results,[{ok:null},{error:'Not implemented'},{error:'unexpected'}]);
 assert.notDeepEqual(results[0],{ok:true});
});
test('execution oracle deduplicates snapshot overlap but keeps distinct same-size executions',()=>{
 const fill=(tradeId)=>({fillId:'legacy-'+tradeId,tradeId,orderId:'7',symbol:'ETH',side:'BUY',price:'100',size:'0.5'});
 const record=(type,fills)=>({type:'ws',text:JSON.stringify({type,channel:'account_all:42',data:{fills}})});
 const account={fills:[]};
 fixLighterFills(account,[record('subscribed/account_all',[fill('old')]),record('update/account_all',[fill('old'),fill('one'),fill('two')]),record('update/account_all',[{...fill('one'),fillId:undefined}])]);
 assert.equal(account.fills.length,2);
 assert.equal(account.fills.reduce((n,fill)=>n+fill.amount,0),1);
});
test('account-scoped history must carry valid auth bound to the requested account',()=>{
 const v=verifier(),token=sample('auth').result;
 for(const route of ['trades','positionFunding','accountInactiveOrders']){
  const url='https://mainnet.zklighter.elliot.ai/api/v1/'+route+'?account_index=42';
  assert.equal(v.request({method:'GET',url,now}),false);
  assert.equal(v.request({method:'GET',url,headers:{authorization:token},now}),true);
  assert.equal(v.request({method:'GET',url:url.replace('42','43'),headers:{authorization:token},now}),false);
 }
 assert.equal(v.request({method:'GET',url:'https://mainnet.zklighter.elliot.ai/api/v1/trades?market_id=1',now}),true);
});
test('each command result binds its own acknowledged IDs, cardinality and order',()=>{
 const v=verifier();
 const batch=new URLSearchParams(form('create-batch'));
 const second=new URLSearchParams({tx_type:'14',tx_info:JSON.parse(batch.get('tx_infos'))[1]}).toString();
 const ack={url,body:'{"code":200,"tx_hash":"ack"}'};
 v.matched(ack,{body:form(),commandIndex:2});v.matched(ack,{body:second,commandIndex:3});
 const command={op:'placeOrders'};
 assert.deepEqual(v.commandResult({ok:['123']},command,2),{ok:['<generated>']});
 assert.deepEqual(v.commandResult({ok:['124']},command,3),{ok:['<generated>']});
 for(const ids of [['123'],[],['124','124']])assert.throws(()=>v.commandResult({ok:ids},command,3),/this command/);
 const b=verifier();b.matched({url:url+'Batch',body:'{"code":200,"tx_hash":["one","two"]}'},{body:form('create-batch'),commandIndex:4});
 assert.deepEqual(b.commandResult({ok:['123','124']},command,4),{ok:['<generated>','<generated>']});
 assert.throws(()=>b.commandResult({ok:['124','123']},command,4),/this command/);
 assert.deepEqual(b.commandResult({ok:[]},command,9),{ok:[]});
});
test('fill oracle inherits market keys and combines both execution sources by canonical market',()=>{
 const metadata={type:'http',method:'GET',url:'https://example.test/api/v1/orderBookDetails',status:200,body:'{"order_book_details":[{"market_id":1,"symbol":"ETH"}]}'};
 const trade={trade_id_str:'9007199254740993',trade_id:9007199254740992,order_id:'7',side:'BUY',price:'100',size:'0.5'};
 const ws=(payload)=>({type:'ws',text:JSON.stringify({type:'update/account_all',channel:'account_all:42',data:payload})});
 for(const payload of [
  {trades:{1:[trade]}},
  {fills:[],trades:{1:[trade]}},
  {fills:[{...trade,symbol:'ETH'}],trades:{1:[trade]}},
 ]){
  const account={fills:[]};fixLighterFills(account,[metadata,ws(payload)]);
  assert.deepEqual(account.fills,[{id:'7',symbol:'ETH',side:'buy',amount:0.5,price:100}]);
 }
});
test('actual transport carries command ownership to acknowledgement validation',async()=>{
 const v=verifier();
 const batch=new URLSearchParams(form('create-batch'));
 const bodies=[form(),new URLSearchParams({tx_type:'14',tx_info:JSON.parse(batch.get('tx_infos'))[1]}).toString()];
 const records=[...bodies.map((requestBody)=>({type:'http',method:'POST',url,requestBody,status:200,body:'{"code":200,"tx_hash":"ack"}'})),{type:'command',t:0,op:'placeOrders'},{type:'command',t:0,op:'placeOrders'}];
 const tape=new Tape(records,{requestBody:lighterRequestBody,verifyRequest:r=>v.request({...r,now}),validateMatchedRequest:(record,r)=>v.matched(record,r)},true);
 await tape.play(undefined,async()=>{},async()=>{const index=tape.commandIndex-1;tape.respond('POST',url,bodies[index]);return [String(123+index)];});
 tape.assertWritesConsumed();
 assert.deepEqual(v.commandResult(tape.results[1],records.at(-1),2),{ok:['<generated>']});
 assert.throws(()=>v.commandResult(tape.results[0],records.at(-1),2),/this command/);
});
test('auth validates canonical integer components and every supplied token',()=>{
 const v=verifier(),token=sample('auth').result;
 for(const index of ['3.0','3e0']){
  const message='1700003600:42:'+index,bytes=Buffer.from(message),padded=Buffer.alloc(Math.ceil(bytes.length/8)*8);bytes.copy(padded);
  const forged=message+':'+oracle.schnorrSign(padded.toString('hex'),options.secret);
  assert.equal(v.auth(forged,now),false);
 }
 const url='https://mainnet.zklighter.elliot.ai/api/v1/accountLimits?account_index=42&auth='+token;
 assert.equal(v.request({method:'GET',url,headers:{authorization:token},now}),true);
 assert.equal(v.request({method:'GET',url,headers:{authorization:'invalid'},now}),false);
 assert.equal(v.request({method:'GET',url,headers:{Authorization:token,authorization:'invalid'},now}),false);
});

for (const name of ['lighter', 'lighter-robinhood']) {
 for (const testnet of [false, true]) {
  test(`${name} ${testnet ? 'testnet' : 'livenet'} independently validates its signed deployment profile`, async () => {
   const vector=(kind)=>corpus.cases.find(c=>c.name===`${name}/${testnet?'testnet':'livenet'}/${kind}`);
   const v=createLighterVerifier({...options,testnet},oracle,name);
   for (const kind of ['create-complete','create-missing-optionals','cancel-large-index','leverage']) {
    const body=vector(kind).wire[0].body;
    assert.equal(v.request({method:'POST',url,body,now}),true,kind);
    const otherChain=createLighterVerifier({...options,testnet:!testnet},oracle,name);
    assert.equal(otherChain.request({method:'POST',url,body,now}),false,`${kind} wrong chain`);
   }
   const adapter=venueOf(name);
   assert.equal(adapter.rust,name);
   assert.equal(adapter.ts.name,name);
   assert.equal(adapter.ts.className,'LighterExchange');
   await adapter.initializeReplay('rust',{...options,testnet});
   assert.equal(adapter.verifyRequest({method:'POST',url,body:vector('create-complete').wire[0].body,now}),true);
   assert.equal(v.auth(vector('auth').result,now),true);
  });
 }
}
test('Robinhood production signatures fail the standard production verifier and vice versa',()=>{
 const standard=createLighterVerifier(options,oracle,'lighter');
 const robinhood=createLighterVerifier(options,oracle,'lighter-robinhood');
 const broker=corpus.cases.find(c=>c.name==='lighter-robinhood/livenet/create-complete');
 assert.equal(standard.request({method:'POST',url,body:broker.wire[0].body,now}),false);
 assert.equal(robinhood.request({method:'POST',url,body:form(),now}),false);
});

test('private tape generator preserves all deployment origins and each mainnet builder default',()=>{
 const directory=mkdtempSync(join(tmpdir(),'lighter-private-profiles-'));
 try {
  for (const [name,testnet,rest,socket] of [
   ['lighter',false,'https://mainnet.zklighter.elliot.ai','wss://mainnet.zklighter.elliot.ai/stream'],
   ['lighter',true,'https://testnet-api.lighter.xyz','wss://testnet.zklighter.elliot.ai/stream'],
   ['lighter-robinhood',false,'https://api.rh.lighter.xyz','wss://api.rh.lighter.xyz/stream'],
   ['lighter-robinhood',true,'https://api.rh-testnet.lighter.xyz','wss://api.rh-testnet.lighter.xyz/stream'],
  ]) {
   const out=join(directory,`${name}-${testnet}.jsonl`);
   execFileSync('corepack',['yarn','-s','rust:private-tape-lighter','--venue',name,...(testnet?['--testnet']:[]),'--out',out],{cwd:fileURLToPath(new URL('../../',import.meta.url))});
   const records=readFileSync(out,'utf8').trim().split('\n').map(line=>JSON.parse(line));
   assert.equal(records.find(r=>r.type==='meta').venue,name);
   assert.equal(records.find(r=>r.type==='config').options.testnet,testnet);
   assert.equal(records.filter(r=>r.type==='command').length,8);
   for (const record of records) {
    if(record.type==='ws')assert.equal(record.url,socket);
    if(record.type!=='http')continue;
    assert.equal(new URL(record.url).origin,rest);
    if(!record.requestBody)continue;
    for (const {type,info} of decodeLighterTransactions(record.requestBody)) {
     if(type!==14)continue;
     assert.deepEqual(info.L2TxAttributes,name==='lighter-robinhood'?{'1':47296,'2':250,'3':0}:testnet?undefined:{'1':158385,'2':250,'3':0});
    }
   }
  }
 } finally { rmSync(directory,{recursive:true,force:true}); }
});
