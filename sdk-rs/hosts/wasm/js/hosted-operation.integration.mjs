/** Explicit proof build: actual EngineHost, actual WASM engine/recipe, actual WASM peer. */
import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {EngineHost} from './host.mjs';
import {createOperationProvider,createOperationPeer} from './hosted-operations.mjs';
import {createKrakenOperationCodec} from './kraken-operation-codec.mjs';
const require=createRequire(import.meta.url);
assert.ok(process.env.HOSTED_OPERATION_WASM,'Set HOSTED_OPERATION_WASM to the nondefault proof build');
const wasm=require(process.env.HOSTED_OPERATION_WASM);
const identity={credential:'origin.credential',signing:'origin.signing'};
function rig(spot,{hold=false,holdFirst=false}={}) {
 let now=9_007_199_254_741,seq=0,sends=0,prepares=0,binds=0,cancels=0,peerCalls=0,release;
 const subject=`hosted-wasm-${spot}-${++rig.serial}`;
 const executor=new wasm.HostedOperationExecutor(JSON.stringify({primary:{key:'fixture-primary',secret:Buffer.from('kraken-test-secret').toString('base64'),stableId:subject}}),spot,false);
 const installed=JSON.parse(executor.binding('primary'));
 const binding={account:subject,network:'mainnet',...installed,generation:subject,metadata:null};
 const peerCodec=createKrakenOperationCodec({wasm,executor,spot});
 const peer=createOperationPeer({bindings:{primary:binding},exchangeName:spot?'krakenspot':'kraken',accountName:subject,codec:{...peerCodec,execute:async(r,c)=>{
   peerCalls++;if(hold || holdFirst && peerCalls===1) await new Promise(resolve=>{release=resolve;});return peerCodec.execute(r,c);
 }},now:()=>now,revision:()=> 'peer-clock'});
 const wires=[],packets=[];
 const provider=createOperationProvider({bindings:{primary:binding},exchangeName:spot?'krakenspot':'kraken',accountName:subject,
  codec:createKrakenOperationCodec({wasm,identity,spot}),capabilities:peer.capabilities,now:()=>now,id:()=>`${subject}-${++seq}`,
  send:(frame,context)=>{wires.push(JSON.parse(frame));return peer.handle(frame,context);}});
 const timers=new Map();let timerId=0;
 const transport={now:()=>now,setTimeout:(fn,ms)=>{const id=++timerId;timers.set(id,{fn,at:now+ms});return id;},clearTimeout:id=>timers.delete(id),
  connect:()=>{throw Error('Unexpected socket');},request:()=>{throw Error('Raw HTTP escaped Claim');},
  prepareHttp:packet=>{prepares++;return {bind:bound=>{binds++;return {ready:()=>true,dispatch:()=>{sends++;packets.push(bound);return Promise.resolve({status:200,body:new TextEncoder().encode('{}')});},cancel:()=>{cancels++;}};},cancel:()=>{cancels++;}};}};
 const engine=wasm.krakenHostedProofOrigin(spot,subject);
 const host=new EngineHost(engine,transport,{}, {signer:provider,signerAlgorithms:['hmacSha512'],ownsEngine:true});
 host.start();
 const command=()=>host.command({op:'fetchAccount'});
 async function tick(){for(let i=0;i<80;i++){await Promise.resolve();for(const [id,t]of [...timers])if(t.at<=now){timers.delete(id);t.fn();}}}
 return {host,peer,executor,command,tick,wires,packets,timers,get sends(){return sends;},get prepares(){return prepares;},get binds(){return binds;},get peerCalls(){return peerCalls;},get cancels(){return cancels;},setNow:value=>{now=value;},release:()=>release?.()};
}
rig.serial=0;
for(const spot of [false,true]) test(`actual ${spot?'Spot':'Futures'} engine reservation travels through peer and final Claim once`,async()=>{
 const r=rig(spot);try {
  const done=r.command();done.catch(()=>{});await r.tick();
  assert.equal(r.sends,1,JSON.stringify({prepares:r.prepares,binds:r.binds,peerCalls:r.peerCalls,wires:r.wires}));await done;assert.equal(r.prepares,1);assert.equal(r.binds,1);assert.equal(r.peerCalls,1);
  const nonce=r.wires[0].request.receipt.nonce;assert.ok(BigInt(nonce)>9007199254740991n);
  assert.equal(r.wires[0].request.descriptor.nonce,nonce);
  assert.ok(!JSON.stringify(r.wires).includes('origin.signing'));assert.ok(!JSON.stringify(r.wires).includes('a3Jha2Vu'));
  const packet=r.packets[0];assert.equal(packet.headers[spot?'API-Key':'APIKey'],'fixture-primary');
  assert.equal(spot ? packet.body : packet.headers.Nonce,spot?`nonce=${nonce}`:nonce);
  assert.equal(Buffer.from(packet.headers[spot?'API-Sign':'Authent'],'base64').length,64);
 } finally {r.host.dispose();r.peer.dispose();r.executor.free();}
 assert.equal(r.timers.size,0);
});
test('real engine timeout cancels held peer work and late settlement cannot send',async()=>{
 const r=rig(false,{hold:true});try{
  const done=r.command();done.catch(()=>{});await r.tick();assert.equal(r.peerCalls,1);assert.equal(r.sends,0);
  r.setNow(9_007_199_254_741+20001);await r.tick();r.release();await r.tick();
  await assert.rejects(done);assert.equal(r.sends,0);
 }finally{r.host.dispose();r.peer.dispose();r.executor.free();}
 assert.equal(r.timers.size,0);
});

test('reversed real Claims repair the older preparation with a new nonce and MAC',async()=>{
 const r=rig(false,{holdFirst:true});try{
  const first=r.command();first.catch(()=>{});await r.tick();assert.equal(r.sends,0);
  const second=r.command();await r.tick();assert.equal(r.sends,1);await second;
  r.release();await r.tick();assert.equal(r.sends,2);await first;
  assert.equal(r.wires.length,3);assert.equal(r.peerCalls,3);
  const [old,newer,repaired]=r.wires.map(w=>w.request);
  assert.ok(BigInt(newer.receipt.nonce)>BigInt(old.receipt.nonce));
  assert.ok(BigInt(repaired.receipt.nonce)>BigInt(newer.receipt.nonce));
  assert.notEqual(repaired.attemptId,old.attemptId);assert.ok(r.cancels>=1);
  assert.equal(r.packets[0].headers.Nonce,newer.receipt.nonce);
  assert.equal(r.packets[1].headers.Nonce,repaired.receipt.nonce);
  assert.notEqual(r.packets[0].headers.Authent,r.packets[1].headers.Authent);
 }finally{r.host.dispose();r.peer.dispose();r.executor.free();}
 assert.equal(r.timers.size,0);
});
