// Actual source callbacks with recording hosts; no live sockets or native gate.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {root,once,declaration,execute,loader} from './source-core.mjs';
const control=process.argv.includes('--control'),selected=process.argv[2];
const read=relative=>fs.readFileSync(path.join(root,relative),'utf8');
const source='packages/safe-cex/src/exchanges/asterdex/';
const pub=read(source+'asterdex.ws-public.ts'),priv=read(source+'asterdex.ws-private.ts');
const load=loader();
const jsonParse=load('packages/json-parse/src/jsonParse.ts').jsonParse;
const types=load('packages/safe-cex/src/types.ts');
const high=JSON.parse('"\\ud800"');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function selectedClass(code,name,members,bindings={},transform=s=>s){
 return execute('class '+name+' {'+members.map(m=>transform(declaration(code,m,name),m)).join('\n')+'}\nexports.Selected='+name+';',bindings).Selected;
}
const profiles={};
profiles['aster-onOpen-turn']=async()=>{
 const flag=declaration(read('packages/safe-cex/src/config/config.ts'),'ENABLE_RECONNECTING_WEBSOCKET');
 const bindings=execute(flag);bindings.WebSocket={OPEN:1};
 const Selected=selectedClass(pub,'AsterDEXPublicWebsocket',['onOpen','subscribe'],bindings,(s,name)=>control&&name==='onOpen'?once(s,'await this.subscribe(allStreams).catch','void this.subscribe(allStreams).catch'):s);
 const receiver=new Selected();Object.assign(receiver,{isDisposed:false,wsReady:false,ws:{readyState:1},subscriptionId:1,topics:{'btcusdt@depth20@100ms':['active']},ping(){},clearOrderBookSubscriptionRetries(){},emitWsReconnected(){}});
 const sent=[];receiver.send=message=>sent.push(message);
 receiver.onOpen();assert.equal(sent.length,1,'ASTER-OPEN immediate aggregate subscription');assert.equal(receiver.wsReady,false,'ASTER-OPEN initial not ready');
 await Promise.resolve();assert.equal(sent.length,2,'ASTER-OPEN replay after first turn');
 assert.equal(receiver.wsReady,false,'ASTER-OPEN second await not ready');
 // The replay catch promise adds a continuation before the outer finally.
 for(let i=0;i<3;i++)await Promise.resolve();
 assert.equal(receiver.wsReady,true,'ASTER-OPEN eventually ready');
 return {profile:'aster-onOpen-turn',immediateSends:1,firstTurnSends:2,firstTurnReady:false,scope:'actual onOpen and subscribe; native single-poll sequence inspected only'};
};
profiles['aster-private-log-cutoff']=()=>{
 const parent=new (selectedClass(read('packages/safe-cex/src/exchanges/base.ts'),'BaseExchange',['log'],types))();
 const events=[];Object.assign(parent,{name:'aster-fixture',options:{tuning:{consoleLogging:false}},emitter:{emit:(...args)=>events.push(args)}});
 const Receiver=selectedClass(priv,'AsterDEXPrivateWebsocket',['onMessage'],{jsonParse,...types},(s)=>control?once(s,'Unhandled private event type: ${json.e}, data: ${JSON.stringify(json).substring(0, 200)}','Unhandled private event type: ${json.e}, data: ${JSON.stringify(json).substring(0, 199)}'):s);
 const receiver=new Receiver();Object.assign(receiver,{isDisposed:false,parent,messageHandlers:{},trackLastReceived(){}});
 const prefix='{"e":"unhandled","x":"';
 const frame=prefix+'a'.repeat(199-prefix.length)+'😀"}';
 receiver.onMessage({data:frame});
 assert.equal(events.length,1,'ASTER-LOG one program event');assert.equal(events[0][0],'log','ASTER-LOG event name');
 const message=events[0][1];
 assert.equal(message.charCodeAt(message.length-1),0xd83d,'ASTER-LOG emitted cutoff unit');
 return {profile:'aster-private-log-cutoff',lastUnit:0xd83d,scope:'actual private onMessage → actual BaseExchange.log → recording program emitter, no human-only diagnostic assumption'};
};
profiles['aster-listen-key-consumer']=async()=>{
 const endpoints=execute(declaration(read(source+'asterdex.types.ts'),'ASTERDEX_ENDPOINTS'));
 const Receiver=selectedClass(priv,'AsterDEXPrivateWebsocket',['LISTEN_KEY_REFRESH_MS','fetchListenKey','connectAndSubscribe'],{...types,...endpoints},(s,name)=>control&&name==='fetchListenKey'?once(s,'return listenKey;',"return 'replacement';"):s);
 const receiver=new Receiver();const urls=[],listeners=[],refresh=[];
 Object.assign(receiver,{isDisposed:false,parent:{options:{testnet:false},getFapiPath:()=>'/fixture-listen-key',primaryAPI:{post:async()=>({data:{listenKey:high}})},log(){}},scheduleListenKeyRefresh:ms=>refresh.push(ms),startListenKeyWatchdog(){},clearConnectRetryTimer(){},scheduleConnectRetry(){throw Error('unexpected retry');},createWebsocket:url=>{urls.push(url);return {addEventListener:(...args)=>listeners.push(args)};}});
 await receiver.connectAndSubscribe();
 assert.equal(urls.length,1,'ASTER-KEY socket callback once');
 assert.equal(urls[0].charCodeAt(urls[0].length-1),0xd800,'ASTER-KEY actual socket URL unit');
 assert.deepEqual(refresh,[1800000],'ASTER-KEY source refresh constant');assert.equal(listeners.length,3,'ASTER-KEY three registrations');
 return {profile:'aster-listen-key-consumer',lastURLUnit:0xd800,scope:'actual fetchListenKey and public connectAndSubscribe; recording socket capability only, no network acceptance'};
};
profiles['aster-public-message-consumer']=()=>{
 const Receiver=selectedClass(pub,'AsterDEXPublicWebsocket',['PING_REQUEST_ID','onMessage','handleTickerUpdate'],{jsonParse},(s,name)=>control&&name==='handleTickerUpdate'?once(s,"this.parent.emitter.emit('ticker', data.s, ticker)","this.parent.emitter.emit('ticker', 'replacement', ticker)"):s);
 const receiver=new Receiver();const ticker={last:1},events=[],updates=[];
 Object.assign(receiver,{isDisposed:false,trackLastReceived(){},parent:{store:{tickersBySymbol:{[high]:ticker},updateTicker:(target,patch)=>{updates.push([target,patch]);Object.assign(target,patch);}},emitter:{emit:(...args)=>events.push(args)}}});
 // Actual parser materializes the escaped unit, actual onMessage dispatches it,
 // actual ticker handler performs a lookup/write before exposing the event key.
 receiver.onMessage({data:'{"stream":"!ticker@arr","data":[{"s":"\\ud800","c":"7"}]}' });
 assert.equal(updates.length,1,'ASTER-PUBLIC actual Store update once');assert.strictEqual(updates[0][0],ticker,'ASTER-PUBLIC Store target identity');assert.equal(ticker.last,7,'ASTER-PUBLIC Store numeric value');
 assert.equal(events.length,1,'ASTER-PUBLIC one event');assert.equal(events[0][0],'ticker','ASTER-PUBLIC ticker event');
 assert.deepEqual(units(events[0][1]),[0xd800],'ASTER-PUBLIC emitted program key unit');
 return {profile:'aster-public-message-consumer',units:[0xd800],last:7,scope:'actual onMessage → actual ticker handler/lookup/write/event; recording Store/emitter, native decoder inspected only'};
};
assert.equal(Object.keys(profiles).length,4,'ASTER-AUDIT independent four-profile floor');
const observations=[];
if(selected){assert.ok(Object.hasOwn(profiles,selected));observations.push(await profiles[selected]());}
else{assert.equal(control,false);for(const run of Object.values(profiles))observations.push(await run());}
process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,observations})+'\n');
