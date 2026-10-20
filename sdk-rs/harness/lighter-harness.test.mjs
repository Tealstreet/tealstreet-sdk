import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {initializeLighterCrypto,createLighterVerifier,decodeLighterTransactions,lighterRequestBody} from './lighter-crypto.mjs';
import {requestKey,subscriptionKeys} from './requests.mjs';
import {Tape} from './tape.mjs';
import {fixLighterFills,fixLighterCommands} from './known-ts-fixes.mjs';
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
 assert.throws(()=>v.commandResult({ok:['999']},{op:'placeOrders'}),/never signed/);
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
  assert.throws(()=>v.commandResult({ok:['123']},{op:'placeOrders'}),/never signed/);
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
