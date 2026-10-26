import assert from 'node:assert/strict';
import test from 'node:test';
import { createOperationPeer, createOperationProvider } from './hosted-operations.mjs';

const binding = {account:'account', network:'mainnet', stableSubject:'owned-subject', expectedCredential:'public-account', generation:'generation', metadata:null};
const work = {receipt:{stampMs:1000,nonce:'9007199254740993'},operations:[],context:{}};
const meta = {task:7,owner:{kind:'managed',request:3,generation:1},deadlineMs:4000,slot:'primary',role:{kind:'read',scope:'account'}};
function pair({execute, now=()=>1000, send, id, revision=()=> 'peer-clock'}={}) {
 let calls=0,serial=0;
 const codec={
  recipes:['kraken.futures.http/1'],
  describe:()=>({kind:'kraken.futures.http',path:'/derivatives/api/v3/accounts'}),
  recipe:d=>d.kind==='kraken.futures.http' ? 'kraken.futures.http/1' : undefined,
  validate:()=>{},
  verify:r=>{assert.equal(r.values.length,3);assert.equal(r.values[0].length,14);assert.equal(r.values[1].length,32);assert.equal(r.values[2].length,64);},
  execute:async (request,context)=>{calls++; return execute ? execute(request,context) : {values:[Array.from(new TextEncoder().encode('public-account')),Array(32).fill(1),Array(64).fill(2)],receipt:request.receipt};},
 };
 const peer=createOperationPeer({bindings:{primary:binding},exchangeName:'kraken',accountName:'fixture',codec,now,revision});
 const provider=createOperationProvider({bindings:{primary:binding},exchangeName:'kraken',accountName:'fixture',codec,now,revision:()=> 'origin-clock',capabilities:{v:2,recipes:codec.recipes,slots:['primary']},send:send??((frame,context)=>peer.handle(frame,context)),id:id??(()=> `id-${++serial}`)});
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

test('all receipt nonce spellings are checked before transport',async()=>{
 let sent=0;const p=pair({send:()=>{sent++;throw Error('unexpected');}});
 for(const nonce of [9007199254740993,'01','+1','1e3','18446744073709551616',null])
  await assert.rejects(p.provider({...work,receipt:{...work.receipt,nonce}},context()));
 assert.equal(sent,0);
});
test('safe task and exact business owner metadata are mandatory',async()=>{
 let sent=0;const p=pair({send:()=>{sent++;throw Error('unexpected');}});
 for(const change of [{task:2**53},{task:-1},{task:0.5},{owner:{kind:'setup',generation:1}},{slot:'secondary',role:{kind:'mutation'}},{owner:{kind:'session',generation:1}}])
  await assert.rejects(p.provider(work,{...context(),meta:{...meta,...change}}));
 assert.equal(sent,0);
});
test('peer clock revision change during provider work discards complete result',async()=>{
 let release,epoch='before';const p=pair({revision:()=>epoch,execute:()=>new Promise(r=>{release=r;})});
 const pending=p.provider(work,context());pending.catch(()=>{});await new Promise(r=>setImmediate(r));
 epoch='after';release({values:[Array(14).fill(1),Array(32).fill(1),Array(64).fill(2)],receipt:work.receipt});
 await assert.rejects(pending);
});
test('peer rejects mismatched bindings, changed roles and unknown envelope fields before callbacks',async()=>{
 const p=pair();let serial=0;
 for(const edit of [d=>{d.v=1;},d=>{d.type='http';},d=>{d.extra='no';},d=>{d.request.binding.generation='other';},d=>{d.request.slot='secondary';},d=>{d.request.role={kind:'mutation',scope:null};}]) {
  const frame={id:`bad-${++serial}`,v:2,type:'op',exchangeName:'kraken',accountName:'fixture',request:{attemptId:serial,deadlineMs:4000,receipt:work.receipt,slot:'primary',role:meta.role,binding,descriptor:{kind:'kraken.futures.http'}}};
  edit(frame);const response=JSON.parse(await p.peer.handle(JSON.stringify(frame)));
  assert.equal(response.error,'invalidRequest');
 }
 assert.equal(p.calls(),0);
});
test('duplicate JSON keys refuse even in an otherwise valid request',async()=>{
 let saved;const p=pair({send:frame=>{saved=frame;throw Error('capture');}});
 await assert.rejects(p.provider(work,context()));
 const duplicate=saved.replace('"id":"id-1"','"id":"id-1","id":"id-2"');
 await assert.rejects(p.peer.handle(duplicate));assert.equal(p.calls(),0);
});
test('response receipt presence, correlation and extra authentication metadata cannot be replaced',async()=>{
 for(const change of [r=>{delete r.result.receipt.nonce;},r=>{r.id='different';},r=>{r.attemptId=8;},r=>{r.result.headers=[['APIKey','other']];},r=>{r.error='refused';}]) {
  let p;p=pair({send:async(f,c)=>{const r=JSON.parse(await p.peer.handle(f,c));change(r);return JSON.stringify(r);}});
  await assert.rejects(p.provider(work,context()));assert.equal(p.calls(),1);
 }
});
test('aborted requests never enter the peer or transport',async()=>{
 let sent=0;const p=pair({send:()=>{sent++;}});const c=new AbortController();c.abort();
 await assert.rejects(p.provider(work,{signal:c.signal,meta}));assert.equal(sent,0);
});
test('reordering receipt keys cannot bypass replay custody',async()=>{
 const p=pair();await p.provider(work,context());
 await assert.rejects(p.provider({...work,receipt:{nonce:work.receipt.nonce,stampMs:work.receipt.stampMs}},context()));
 assert.equal(p.calls(),1);
});
test('family codec owns an eleven-value batch and checks the peer clock at execution',async()=>{
 const r={stampMs:1000,expiryMs:2000};let checked=0;
 const codec={recipes:['paradex.orderBatch/1'],recipe:()=> 'paradex.orderBatch/1',describe:()=>({kind:'paradex.orderBatch',items:Array(10).fill({})}),validate:()=>{},
  execute:async(req,c)=>{await Promise.resolve();assert.equal(c.check(),1000);checked++;return {receipt:req.receipt,values:[Array(14).fill(1),...Array.from({length:10},()=>Array(64).fill(2))]};},
  verify:value=>{assert.equal(value.values.length,11);for(const signature of value.values.slice(1))assert.equal(signature.length,64);},
 };
 const options={bindings:{primary:binding},exchangeName:'paradex',accountName:'batch',codec,now:()=>1000};
 const peer=createOperationPeer({...options,revision:()=> 'clock'});
 const provider=createOperationProvider({...options,capabilities:peer.capabilities,send:(f,c)=>peer.handle(f,c)});
 const result=await provider({...work,receipt:r},{...context(),meta:{...meta,role:{kind:'mutation'}}});
 assert.equal(result.values.length,11);assert.equal(checked,1);
});

test('duplicate correlation refusal cannot remove another live provider owner',async()=>{
 let release,sent=0,frame;const p=pair({id:()=> 'fixed',send:f=>{sent++;frame=f;return new Promise(r=>{release=r;});}});
 const first=p.provider(work,context());first.catch(()=>{});await new Promise(r=>setImmediate(r));
 for(let i=0;i<2;i++)await assert.rejects(p.provider(work,{...context(),meta:{...meta,task:8+i}}));
 assert.equal(sent,1);release(await p.peer.handle(frame));await first;
});
