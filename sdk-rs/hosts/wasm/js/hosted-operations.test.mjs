import assert from 'node:assert/strict';
import test from 'node:test';
import { createOperationPeer, createOperationProvider } from './hosted-operations.mjs';

const binding = {account:'account', network:'mainnet', stableSubject:'owned-subject', expectedCredential:'public-account', generation:'generation', metadata:null};
const work = {receipt:{stampMs:1000,nonce:'9007199254740993'},operations:[],context:{}};
const meta = {task:7,owner:{kind:'managed',request:3,generation:1},deadlineMs:4000,slot:'primary',role:{kind:'read',scope:'account'}};
function pair({execute, now=()=>1000, send}={}) {
 let calls=0;
 const codec={
  recipes:['kraken.futures.http/1'],
  describe:()=>({kind:'kraken.futures.http',path:'/derivatives/api/v3/accounts'}),
  recipe:d=>d.kind==='kraken.futures.http' ? 'kraken.futures.http/1' : undefined,
  validate:()=>{},
  verify:r=>{assert.equal(r.values.length,3);assert.equal(r.values[0].length,14);assert.equal(r.values[1].length,32);assert.equal(r.values[2].length,64);},
  execute:async (request,context)=>{calls++; return execute ? execute(request,context) : {values:[Array.from(new TextEncoder().encode('public-account')),Array(32).fill(1),Array(64).fill(2)],receipt:request.receipt};},
 };
 const peer=createOperationPeer({binding,exchangeName:'kraken',accountName:'fixture',codec,now,revision:()=> 'peer-clock'});
 const provider=createOperationProvider({binding,exchangeName:'kraken',accountName:'fixture',codec,now,revision:()=> 'origin-clock',capabilities:{v:2,recipes:codec.recipes,slots:['primary']},send:send??((frame,context)=>peer.handle(frame,context)),id:()=> 'id'});
 return {provider,peer,calls:()=>calls};
}
const context=()=>({signal:new AbortController().signal,meta});
test('shared provider and real peer preserve full receipt with canonical large nonce',async()=>{
 const p=pair();let result;await assert.doesNotReject(async()=>{result=await p.provider(work,context());});
 assert.deepEqual(result.receipt,work.receipt);assert.equal(p.calls(),1);
});
test('duplicate task and receipt under a fresh id refuses without second callback',async()=>{
 const p=pair();await assert.doesNotReject(p.provider(work,context()));
 await assert.rejects(p.provider(work,context()));assert.equal(p.calls(),1);
});
test('missing metadata and unnegotiated slot refuse before transport',async()=>{
 let sent=0;const p=pair({send:()=>{sent++;throw Error('unexpected');}});
 await assert.rejects(p.provider(work,{signal:new AbortController().signal}));
 await assert.rejects(p.provider(work,{...context(),meta:{...meta,slot:'secondary'}}));assert.equal(sent,0);
});
test('deadline equality during asynchronous callback never publishes a result',async()=>{
 let time=1000,release;const p=pair({now:()=>time,execute:()=>new Promise(r=>{release=r;})});
 const pending=p.provider(work,context());pending.catch(()=>{});await new Promise(r=>setImmediate(r));assert.equal(typeof release,'function');
 time=4000;release({values:[Array(14).fill(1),Array(32).fill(1),Array(64).fill(2)],receipt:work.receipt});
 await assert.rejects(pending);assert.equal(p.calls(),1);
});
test('peer disposal with unresolved callback cannot publish late results',async()=>{
 let release;const p=pair({execute:()=>new Promise(r=>{release=r;})});
 const pending=p.provider(work,context());pending.catch(()=>{});await new Promise(r=>setImmediate(r));assert.equal(typeof release,'function');p.peer.dispose();
 release({values:[Array(14).fill(1),Array(32).fill(1),Array(64).fill(2)],receipt:work.receipt});
 await assert.rejects(pending);assert.equal(p.calls(),1);
});
