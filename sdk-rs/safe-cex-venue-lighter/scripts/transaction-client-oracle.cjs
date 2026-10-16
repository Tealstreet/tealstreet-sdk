'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const source=fs.readFileSync(path.join(root,'packages/lighter-sdk/src/client/transaction-client.ts'),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0);
const NodeTextEncoder=TextEncoder;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function encode(v){if(v===undefined)return{undefined:true};if(typeof v==='bigint')return{bigint:v.toString()};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:b.toString('hex')}}if(typeof v==='string')return{utf16:units(v)};if(Array.isArray(v))return v.map(encode);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));return v;}
const config={chainId:304,accountIndex:9007199254740993n,apiKeyIndex:13},privateKey='key\ud800-A';
const order={orderId:301n,marketId:17,isBuy:true,price:2401n,amount:2413n};
const group={groupingType:1,orders:[order,{...order,orderId:302n,amount:0n,isBuy:false,orderType:'stop_loss',reduceOnly:true}]};
const approval={integratorAccountIndex:501n,maxPerpsTakerFee:17,maxPerpsMakerFee:43,maxSpotTakerFee:89,maxSpotMakerFee:251,approvalExpiry:1710000000000n,l1Signature:'L1\ud800👩'};
const batchResponse={txHashes:['hash-A\ud800','hash-B👩'],predictedExecutionTimeMs:17.25,volumeQuotaRemaining:'quota\udfff'};
const flush=async()=>{for(let i=0;i<100;i++)await Promise.resolve();}; // microtask enumeration floor stops at100
function harness(mode={}){
 const trace=[],counts={},token=Object.assign(new Error('host\ud800'),{name:'Host\udfff'});let clocks=0,client;
 const log=(method,args=[])=>{trace.push({method,args:encode(args)});counts[method]=(counts[method]||0)+1;};
 const crypto={isReady(){log('ready');if(mode.kind==='ready-disable'){global.TextEncoder=undefined;contextGlobal.Buffer=undefined;}return mode.kind!=='ready-false'},poseidon2Hash(input){log('hash',[input]);return `hash-${counts.hash}\ud800`},poseidon2HashTwoToOne(left,right){log('aggregate',[left,right]);return `aggregate-${counts.aggregate}\ud800`},schnorrSign(input,key){log('sign',[input,key]);return 'signature\udfff'},get poseidon2HashFp5(){log('getFp5');return function(input){log('fp5',[input]);return counts.fp5===1?'0xbase\ud800':counts.fp5===2?'0Xattrs\udfff':'fp5-final👩';}}};
 const api={getNextNonce(account,key){log('nonce',[account,key]);return mode.kind==='nonce-error'?Promise.reject(token):Promise.resolve(71n)},sendRawTx(tx){log('send',[tx]);return mode.kind==='send-error'?Promise.reject(token):Promise.resolve('sent\ud800👩')},sendRawTxBatch(txs){log('batch',[txs]);return mode.kind==='batch-error'?Promise.reject(token):Promise.resolve(batchResponse)}};
 const context={exports:{},TextEncoder:mode.encoder==='unavailable'||mode.encoder==='buffer'?undefined:NodeTextEncoder,Buffer:mode.encoder==='unavailable'?undefined:Buffer,Date:{now(){log('clock',[client.cachedNonce]);return 1700000000000+7*clocks++;}}};
 const contextGlobal={Buffer:context.Buffer};global.TextEncoder=context.TextEncoder;new Function('exports','globalThis','Date',compiled.outputText)(context.exports,contextGlobal,context.Date);client=new context.exports.TransactionClient({...config,httpClient:api,crypto,privateKey});
 return {client,api,trace,token,counts,get clocks(){return clocks}};
}
const cases=[];
async function capture(method,args,mode={}){
 const h=harness(mode);let output;
 try{output={ok:encode(await h.client[method](...args))}}catch(e){output=e===h.token?{thrown:true}:{error:{name:e.name,units:units(e.message)}};}
 cases.push({method,args:encode(args),mode,output,trace:h.trace,cachedNonce:encode(h.client.cachedNonce),clockCalls:h.clocks});
}
(async()=>{
 const inputs={CreateOrder:[order],CreateOrders:[[order,{...order,orderId:303n,price:2449n}]],CreateGroupedOrders:[group],CancelOrder:[{orderId:401n,marketId:23}],Transfer:[{toAccountIndex:601n,amount:701n,tokenId:-0}],UpdateLeverage:[{marketId:31,leverage:7.5,marginMode:-0}],ApproveIntegrator:[approval]};
 for(const [suffix,args]of Object.entries(inputs))for(const prefix of ['get','submit']){
  const name=prefix+suffix+(prefix==='get'?(suffix==='CreateOrders'?'Transactions':'Transaction'):'');
  // Source plural public getter is getCreateOrderTransactions.
  const method=name==='getCreateOrdersTransactions'?'getCreateOrderTransactions':name;
  await capture(method,args);await capture(method,args,{kind:'nonce-error'});
 }
 for(const encoder of ['text','buffer','unavailable'])await capture('getAuthToken',[3601n],{encoder});
 await capture('getAuthToken',[3601n],{kind:'ready-false'});
 await capture('getAuthToken',[3601n],{kind:'ready-disable'});
 await capture('getApproveIntegratorTransaction',[{...approval,nonce:7n}],{kind:'nonce-error'});
 await capture('getCreateOrderTransactions',[[]]);await capture('submitCreateOrders',[[]]);
 await capture('getCreateOrderTransactions',[[order,{...order,price:-1n}]]);
 await capture('submitTransfer',inputs.Transfer,{kind:'send-error'});
 await capture('submitCreateOrders',inputs.CreateOrders,{kind:'batch-error'});
 await capture('getCancelOrderTransaction',inputs.CancelOrder,{kind:'ready-false'});
 await capture('getCreateGroupedOrdersTransaction',[{...group,integratorAccountIndex:501n,integratorMakerFee:17,integratorTakerFee:43}]);
 assert.equal(cases.length,41,'independent public case floor');
 const nonceCases=[];
 for(const scenario of ['fifo-cache','concurrent-fetch','fetch-reject','refresh-reject','reset-inflight','drop-observer','reset-queued']){
  const h=harness(),outputs=[];let held;
  const observe=(p,label)=>p.then(v=>outputs.push({label,ok:encode(v)}),e=>outputs.push({label,thrown:e===h.token}));
  if(scenario==='fifo-cache'){
   h.client.cachedNonce=40n;const p=h.client.getCreateOrderTransaction(order),q=h.client.getCreateOrderTransaction({...order,orderId:303n});await Promise.all([observe(p,'a'),observe(q,'b')]);
  }else{
   if(scenario==='refresh-reject'||scenario==='reset-queued')h.client.cachedNonce=20n;
   h.api.getNextNonce=(account,key)=>{h.trace.push({method:'nonce',args:encode([account,key])});h.counts.nonce=(h.counts.nonce||0)+1;
    if((scenario==='fetch-reject'||scenario==='refresh-reject')&&h.counts.nonce===1)return Promise.reject(h.token);
    if(scenario==='concurrent-fetch'||scenario==='reset-inflight'||scenario==='drop-observer')return new Promise(resolve=>{held=resolve});
    return Promise.resolve(71n);};
   const p=h.client.getNextNonce(scenario==='refresh-reject');
   if(scenario!=='drop-observer')observe(p,'a');
   if(scenario!=='reset-inflight')observe(h.client.getNextNonce(),'b');
   if(scenario==='concurrent-fetch')observe(h.client.getNextNonce(),'c');
   if(scenario==='reset-queued')h.client.resetNonce();
   await flush();
   if(held){if(scenario==='reset-inflight')h.client.resetNonce();held(9n);}
   await flush();
  }
  nonceCases.push({scenario,outputs,trace:h.trace,cachedNonce:encode(h.client.cachedNonce),clockCalls:h.clocks});
 }
 assert.equal(nonceCases.length,7);
 const ownership=harness();const changed={...order};const pending=ownership.client.getCreateOrderTransaction(changed);changed.price=99n;const sourceChanged=await pending;
 const identity=harness(),p=identity.client.getCancelOrderTransaction({orderId:401n,marketId:23});let first;const checks=[];
 p.then(v=>{first=v;v.cancelDetails.orderId=777n;});p.then(v=>checks.push({sameObject:v===first,orderId:encode(v.cancelDetails.orderId)}));await flush();
 const batchId=harness();batchId.api.sendRawTxBatch=()=>Promise.resolve(batchResponse);const hashes=await batchId.client.submitCreateOrders([]);checks.push({sameHashArray:hashes===batchResponse.txHashes});
 assert.deepEqual(checks,[{sameObject:true,orderId:encode(777n)},{sameHashArray:true}]);
 const batchTypeRecords=[{txHashes:[]},{txHashes:['A\ud800','B👩','A\ud800'],predictedExecutionTimeMs:-0,volumeQuotaRemaining:Infinity},{txHashes:['C','D'],predictedExecutionTimeMs:NaN,volumeQuotaRemaining:'quota\udfff'}].map(encode);
 const tree=ts.createSourceFile('http-types.ts',fs.readFileSync(path.join(root,'packages/lighter-sdk/src/client/http-types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
 assert.equal(tree.parseDiagnostics.length,0,'batch response source parse');
 const decl=tree.statements.filter(n=>ts.isInterfaceDeclaration(n)&&n.name.text==='SendTxBatchResponse');assert.equal(decl.length,1);assert.equal((decl[0].heritageClauses||[]).length,0,'batch response no inherited schema');for(const member of decl[0].members)assert.ok(ts.isPropertySignature(member),'batch response property fields');assert.deepEqual(decl[0].members.map(n=>[n.name.getText(tree),n.type.getText(tree),!!n.questionToken]),[['txHashes','string[]',false],['predictedExecutionTimeMs','number',true],['volumeQuotaRemaining','number | string',true]],'batch response source schema');
 const body={config:encode(config),privateKey:encode(privateKey),cases,nonceCases,batchTypeRecords,identity:checks,ownershipFinding:{sourcePrice:encode(sourceChanged.orderDetails.price),nativeSnapshotPrice:encode(order.price)}};
 process.stdout.write(JSON.stringify({contentHash:require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex'),...body}));
})().catch(e=>{process.stderr.write(String(e.stack||e));process.exitCode=1});
