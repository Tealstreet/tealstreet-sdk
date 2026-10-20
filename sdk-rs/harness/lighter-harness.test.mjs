import {venueOf} from './venues.mjs';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,rmSync} from 'node:fs';
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {initializeLighterCrypto,createLighterVerifier,decodeLighterTransactions,lighterRequestBody} from './lighter-crypto.mjs';
import {requestKey,subscriptionKeys} from './requests.mjs';
import {Tape} from './tape.mjs';
import {Responses} from './responses.mjs';
import {parseRequestJson} from './request-json.mjs';
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
 assert.equal(tape.unusedWrites,1);
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

// The wire must not choose different first/last duplicate values than the verifier.
test('Lighter form matching retains duplicate and unexpected form fields', () => {
 const venue = {requestBody:lighterRequestBody};
 const key = (body) => requestKey('POST',url,body,new Set(),venue);
 const canonical = key(form());
 const duplicate = new URLSearchParams(form());
 duplicate.append('tx_info',duplicate.get('tx_info'));
 assert.notEqual(key(duplicate),canonical);
 assert.notEqual(key(duplicate.toString()),canonical);
 const extra = new URLSearchParams(form());extra.set('unexpected_economic_field','1');
 assert.notEqual(key(extra),canonical);
 assert.notEqual(key(extra.toString()),canonical);
 const tape = new Tape([{type:'http',method:'POST',url,requestBody:form(),status:200,body:'{"code":200}'}],venue);
 for (const body of [duplicate.toString(),extra.toString()]) {
  assert.equal(tape.respond('POST',url,body).status,599);
  assert.equal(tape.missingWrites,1);
 }
 assert.equal(tape.respond('POST',url,form()).status,200);
});

test('Robinhood production signatures fail the standard production verifier and vice versa',()=>{
 const standard=createLighterVerifier(options,oracle,'lighter');
 const robinhood=createLighterVerifier(options,oracle,'lighter-robinhood');
 const broker=corpus.cases.find(c=>c.name==='lighter-robinhood/livenet/create-complete');
 assert.equal(standard.request({method:'POST',url,body:broker.wire[0].body,now}),false);
 assert.equal(robinhood.request({method:'POST',url,body:form(),now}),false);
});

test('private tape generator preserves all deployment origins and disables absent builder defaults',()=>{
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
     assert.deepEqual(info.L2TxAttributes,name==='lighter'&&!testnet?{'1':158385,'2':250,'3':0}:undefined);
    }
   }
  }
 } finally { rmSync(directory,{recursive:true,force:true}); }
});

// Exercise the exact production constructor and outbound override, not only SharedTape.
const productionTape = (name, records, venue) => {
 const source=readFileSync(new URL('./replay.mjs',import.meta.url),'utf8');
 const start=source.indexOf('const VOLATILE =');
 const classStart=source.indexOf('class Tape extends SharedTape {');
 const end=source.indexOf('\nconst settle =',classStart);
 assert(start>=0 && classStart>start && end>classStart);
 const ProductionTape=new Function('SharedTape','values','venue','records','bookSymbols','callerIds',
  'keyOf','Responses','sharedUrl','parseRequestJson','isRead','isPrivate',
  `${source.slice(start,classStart)}\nreturn (${source.slice(classStart,end)});`)(
   Tape,{venue:name},venue,records,[],new Set(),requestKey,Responses,undefined,parseRequestJson,
   (method)=>method==='GET',true);
 return new ProductionTape('ts');
};
const jsonNumbers=(value)=>JSON.stringify(value,(_key,item)=>typeof item==='bigint'?JSON.rawJSON(String(item)):item);
const signedForm=(name,kind='create-complete',edit=()=>{})=>{
 const item=corpus.cases.find((item)=>item.name===`${name}/livenet/${kind}`);
 const {type,info}=decodeLighterTransactions(item.wire[0].body)[0];
 edit(info);
 const words=(...values)=>Buffer.concat(values.map((value)=>{const bytes=Buffer.alloc(8);bytes.writeBigUInt64LE(BigInt(value));return bytes;})).toString('hex');
 const unwrap=(value)=>{assert.equal(typeof value,'string');return value.replace(/^0x/,'');};
 let preimage=words(name==='lighter-robinhood'?466324:304,type,info.Nonce,info.ExpiredAt,info.AccountIndex,info.ApiKeyIndex);
 if(type===14){
  preimage+=words(info.MarketIndex,info.ClientOrderIndex,info.BaseAmount,info.Price,info.IsAsk,info.Type,info.TimeInForce,info.ReduceOnly,info.TriggerPrice,info.OrderExpiry);
  if(info.L2TxAttributes){
   const attrs=Object.entries(info.L2TxAttributes).filter(([,value])=>value!==0).flatMap(([kind,value])=>[Number(kind),value]);
   while(attrs.length<8)attrs.push(0);
   preimage=unwrap(oracle.poseidon2HashFp5(preimage))+unwrap(oracle.poseidon2HashFp5(words(...attrs)));
  }
 }else if(type===15)preimage+=words(info.MarketIndex,info.Index);
 else throw new Error('Unsupported test transaction');
 info.Sig=Buffer.from(unwrap(oracle.schnorrSign(preimage,options.secret)),'hex').toString('base64');
 return new URLSearchParams({tx_type:String(type),tx_info:jsonNumbers(info)}).toString();
};
const ackRecord=(route,requestBody,extra={})=>({type:'http',method:'POST',url:route,requestBody,status:200,body:'{"code":200,"tx_hash":"ack"}',...extra});
const lighterProduction=async(name,requestBody)=>{
 const venue=venueOf(name);
 await venue.initializeReplay('rust',options);
 const route=name==='lighter-robinhood'?'https://api.rh.lighter.xyz/api/v1/sendTx':url;
 return {tape:productionTape(name,[ackRecord(route,requestBody)],venue),route};
};

test('production outbound reuses each SharedTape comparison key with custom contracts and rejection audits',()=>{
 const route='https://identity.test/write';
 const venue={rust:'lighter-robinhood',isWrite:(method)=>method==='POST',
  requestKey:(method,url,body,ids)=>'CUSTOM '+requestKey(method,url,body,ids),
  verifyRequest:({headers})=>headers?.verified===true};
 const tape=productionTape('lighter-robinhood',[
  ackRecord(route,{quantity:1},{tsRequestBody:{quantity:'1'}}),ackRecord(route,{quantity:2}),
  {type:'http',method:'GET',url:route,status:200,body:'{}'},
 ],venue);
 assert.equal(tape.respond('POST',route,{quantity:'1'},{verified:true}).status,200);
 assert.equal(tape.outbound[0],tape.writes[0]);
 assert.match(tape.outbound[0],/^CUSTOM /);
 assert.equal(tape.outbound[0],venue.requestKey('POST',route,{quantity:1},new Set()));
 assert.equal(tape.respond('GET',route,undefined,{verified:true}).status,200);
 assert.equal(tape.outbound.length,1);
 assert.throws(()=>tape.respond('POST',route,{quantity:2},{verified:false}),/invalid replay signature/);
 assert.equal(tape.invalidSignatures,1);assert.equal(tape.writes.length,1);assert.equal(tape.outbound.length,1);
 assert.equal(tape.respond('POST',route,{quantity:99},{verified:true}).status,599);
 assert.equal(tape.missingWrites,1);
 assert.equal(tape.outbound[1],tape.writes[1]);
 assert.notEqual(tape.outbound[1],tape.outbound[0]);
 assert.equal(tape.respond('POST',route,{quantity:2},{verified:true}).status,200);
 assert.equal(tape.missingWrites,0);
 assert.equal(tape.outbound[2],tape.writes[2]);
 assert.notEqual(tape.outbound[2],tape.outbound[1]);
});

for(const name of ['lighter','lighter-robinhood']){
 test(`${name} production accepts independently verified fresh expiry differences with identical outbound identity`,async()=>{
  const oldNow=Date.now;Date.now=()=>now;
  try{
   const expected=signedForm(name,'cancel-large-index');
   const {venueOf:freshVenueOf}=await import(`./venues.mjs?uninitialized-${name}`);
   const venue=freshVenueOf(name),route=name==='lighter-robinhood'?'https://api.rh.lighter.xyz/api/v1/sendTx':url;
   // A fresh profile closure cannot inherit another test's initialized verifier.
   const uninitialized=productionTape(name,[ackRecord(route,expected)],venue);
   assert.throws(()=>uninitialized.respond('POST',route,expected),/invalid replay signature/);
   assert.equal(uninitialized.missingWrites,1);assert.deepEqual(uninitialized.outbound,[]);
   const baseline=await lighterProduction(name,expected);
   assert.equal(baseline.tape.respond('POST',route,expected).status,200);
   const fresh=signedForm(name,'cancel-large-index',(info)=>{info.ExpiredAt+=293;});
   const v=createLighterVerifier(options,oracle,name);
   assert.equal(v.request({method:'POST',url:route,body:fresh,now}),true);
   const {tape}=await lighterProduction(name,expected);
   assert.equal(tape.respond('POST',route,fresh).status,200);
   assert.deepEqual(tape.outbound,baseline.tape.outbound);
   assert.deepEqual(tape.outbound,tape.writes);
   assert.equal(tape.missingWrites,0);
   assert.equal(tape.respond('POST',route,fresh).status,599,'an ACK is single use');
   assert.equal(tape.unserved,1);
  }finally{Date.now=oldNow;}
 });
 test(`${name} production rejects freshly signed economic target nonce and unknown-field mutations`,async()=>{
  const oldNow=Date.now;Date.now=()=>now;
  try{
   for(const kind of ['create-complete','cancel-large-index']){
    const expected=signedForm(name,kind);
    const {tape,route}=await lighterProduction(name,expected);
    const v=createLighterVerifier(options,oracle,name);
    const edits=kind==='create-complete'?
     [(i)=>{i.Price+=1;},(i)=>{i.BaseAmount+=1;},(i)=>{i.MarketIndex+=1;},(i)=>{i.Nonce+=1;}]:
     [(i)=>{i.Index+=1n;},(i)=>{i.MarketIndex+=1;},(i)=>{i.Nonce+=1;}];
    for(const edit of [...edits,(i)=>{i.unknownEconomicField=1;},(i)=>{i.signature='unknown';},(i)=>{i.clientOrderId='caller-looking';}]){
     const changed=signedForm(name,kind,edit);
     assert.equal(v.request({method:'POST',url:route,body:changed,now}),true,'independent crypto accepts re-signed control');
     assert.equal(tape.respond('POST',route,changed).status,599);
     assert.equal(tape.missingWrites,1);
     assert.equal(tape.outbound.at(-1),tape.writes.at(-1));
    }
    for(const key of ['signature','timestamp','recvWindow','requestId','clientOrderId','newClientOrderId','clientAlgoId','unknownEconomicField']){
     const changedUrl=route+'?'+new URLSearchParams({[key]:'unexpected-business-field'});
     assert.equal(v.request({method:'POST',url:changedUrl,body:expected,now}),true);
     assert.equal(tape.respond('POST',changedUrl,expected).status,599,key);
     assert.equal(tape.missingWrites,1,key);
    }
    for(const edit of [(f)=>f.append('tx_info',f.get('tx_info')),(f)=>f.set('unknownEconomicField','1')]){
     const changed=new URLSearchParams(expected);edit(changed);
     assert.equal(tape.respond('POST',route,changed.toString()).status,599);
     assert.equal(tape.missingWrites,1);
    }
    assert.equal(tape.respond('POST',route,expected).status,200);
    assert.equal(tape.missingWrites,0);
   }
  }finally{Date.now=oldNow;}
 });
 test(`${name} production rejects invalid signature account key and expiry before appending or consuming`,async()=>{
  const oldNow=Date.now;Date.now=()=>now;
  try{
   const expected=signedForm(name,'cancel-large-index');
   const {tape,route}=await lighterProduction(name,expected);
   for(const changed of [
    rewrite(expected,(i)=>{i.Sig=Buffer.alloc(80).toString('base64');}),
    signedForm(name,'cancel-large-index',(i)=>{i.AccountIndex+=1;}),
    signedForm(name,'cancel-large-index',(i)=>{i.ApiKeyIndex+=1;}),
    signedForm(name,'cancel-large-index',(i)=>{i.ExpiredAt=now-1;}),
   ]){
    assert.throws(()=>tape.respond('POST',route,changed),/invalid replay signature/);
    assert.equal(tape.missingWrites,1);assert.deepEqual(tape.outbound,[]);assert.deepEqual(tape.writes,[]);
   }
   assert.equal(tape.invalidSignatures,4);
   assert.equal(tape.respond('POST',route,expected).status,200);
   assert.equal(tape.missingWrites,0);
  }finally{Date.now=oldNow;}
 });
}
