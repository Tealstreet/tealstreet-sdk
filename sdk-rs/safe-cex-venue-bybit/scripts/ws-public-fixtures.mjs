import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
import chunk from 'lodash/chunk.js';
import flatten from 'lodash/flatten.js';
const source=ts.createSourceFile('public.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitPublicWebsocket');
const names=['onMessage','subscribe','ping','sendOhlcvDiagnosticPayload','emitOhlcvDiagnosticReceive'];
const methods=klass.members.filter(m=>names.includes(m.name?.getText(source)));
assert.equal(methods.length,names.length);
const context=vm.createContext({performance:{now:()=>1000},chunk,flatten,jsonParse:data=>{try{return JSON.parse(data)}catch{return null}},sleep:async delay=>context.delays.push(delay)});
vm.runInContext(ts.transpileModule(`class Venue {${methods.map(m=>m.getText(source)).join('\n')}}; globalThis.venue=new Venue();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const dispatch=[];
for(const handlers of [{},{pong:1},{tickers:2},{'tickers.BTCUSDT':1,tickers:1,pong:1},{'tickers.BTCUSDT':0,tickers:1},{kline:1,'kline.1.BTCUSDT':1},{'tickers.BTCUSDT':2,pong:0}])
 for(const disposed of [false,true]) for(const data of ['{"topic":"tickers.BTCUSDT","data":{"lastPrice":"1"}}','{"topic": "tickers.BTCUSDT","data":{}}','{"topic":"tickers.BTCUSDT",','{"topic":""}','{"topic":"tickers.BTCUSDT"','{"topic":"kline.1.BTCUSDT","data":[]}','{"topic":".tickers"}','{"op":"pong"}','not-json pong','{"topic":"not-tickers.BTCUSDT"}','{"topic":"tickers.BTCUSDT_extra"}','{"nested":{"topic":"tickers.BTCUSDT"}}','{"topic":"tickers.BTCUSDT","data":{"n":9007199254740993,"large":1e21,"tiny":1e-8}}','{"topic":"unhandled","pong":true}','{"topic":"tickers.BTCUSDT","note":"pong"}','{"topic":"tickers.BTCUSDT\\"extra","data":{}}']) {
 const calls=[],stats=[];const v=context.venue;
 v.isDisposed=disposed;v.trackLastReceived=()=>{};v.directFallback={onMessage(){}};v.emitOhlcvDiagnosticReceive=()=>{};
 v.recordMessageHandlerStats=key=>stats.push(key);v.messageHandlers={};
 for(const [key,count] of Object.entries(handlers))v.messageHandlers[key]=Array.from({length:count},(_,n)=>value=>calls.push({handler:`${key}:${n}`,value}));
 v.onMessage({data});dispatch.push({handlers,disposed,data,calls,stats:stats.sort()});
}
const subscriptions=[];
for(const count of [0,1,9,10,11,20,21,51]) {
 const topics={tickers:Array.from({length:count},(_,i)=>`tickers.S${i}`),'kline.1.BTCUSDT':'kline.1.BTCUSDT'};
 for(const entry of [topics,{z:['z1','z2'],10:'ten',2:['two'],a:'a'},{}]) {
  const messages=[];context.delays=[];context.venue.topics=entry;context.venue.send=message=>messages.push(message);
  await context.venue.subscribe();subscriptions.push({topics:entry,messages,delays:context.delays});
 }
}
const sends=[];
const privateSource=ts.createSourceFile('private.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const privateClass=privateSource.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitPrivateWebsocket');
const privateMethods=privateClass.members.filter(m=>['ping','subscribe'].includes(m.name?.getText(privateSource)));
assert.equal(privateMethods.length,2);
vm.runInContext(ts.transpileModule(`class Private {${privateMethods.map(m=>m.getText(privateSource)).join('\n')}}; globalThis.private=new Private();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
for(const disposed of [false,true])for(const method of ['publicPing','privatePing','privateSubscribe','diagnosticSend']) {
 const messages=[];let diagnosticSends=0;const v=method.startsWith('private')?context.private:context.venue;
 v.isDisposed=disposed;v.send=message=>{if(!disposed)messages.push(message)};v.emitWsDiagnosticSend=message=>{if(!message.includes('ping'))diagnosticSends++;};v.sendWithLog=message=>{v.send(message);v.emitWsDiagnosticSend(message);};
 const op='unsubscribe',topics=['kline.1.BTCUSDT','publicTrade.BTCUSDT'];
 if(method==='diagnosticSend')v.sendOhlcvDiagnosticPayload({op,args:topics});else if(method==='privateSubscribe')v.subscribe();else v.ping();
 sends.push({disposed,method,messages,op,topics,diagnosticSends});
}
const diagnostics=[];
for(const frame of [null,false,[],{}, {topic:'kline.1.BTCUSDT'}, {topic:'publicTrade.BTCUSDT'}, {topic:'tickers.BTCUSDT'}, {op:'subscribe'}, {op:'unsubscribe'}, {op:'other'}, {topic:4}, {topic:['kline.1']}, {topic:'kline'}, {topic:'.kline.'}]) {
 const data=JSON.stringify(frame);let allowed;
 context.venue.emitWsDiagnosticReceive=(raw,opts)=>{assert.equal(raw,data);allowed=opts.allowChannel(JSON.parse(raw));};
 // Earlier dispatch cases intentionally replace this method on the instance.
 const member=methods.find(m=>m.name.getText(source)==='emitOhlcvDiagnosticReceive');
 // Arrow initializer uses lexical this, so bind the receiving object in a wrapper.
 vm.runInContext(ts.transpileModule(`globalThis.runDiag = function(data) { const callback = ${member.initializer.getText(source)}; callback(data); };`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 context.runDiag.call(context.venue,data);
 diagnostics.push({data,allowed});
}
const divergences=[];
for(const [kind,data] of [['prototype','{"topic":"constructor"}'],['overflow','{"topic":"tickers.BTCUSDT","data":1e400}'],['surrogate','{"topic":"tickers.BTCUSDT","data":"\\ud800"}']]) {
 const v=context.venue;let called=0;v.isDisposed=false;v.messageHandlers={tickers:[()=>called++]};v.recordMessageHandlerStats=()=>{};v.emitOhlcvDiagnosticReceive=()=>{};
 let error;try{v.onMessage({data})}catch(e){error=e.message}
 if(kind==='prototype')assert.match(error,/not a function/);else {assert.equal(error,undefined);assert.equal(called,1);}
 divergences.push({kind,data});
}
// Observe the whole live method before JSON erases inherited functions.
const prototypeDispatch=[];
const protoKeys=Object.getOwnPropertyNames(Object.prototype);
assert.equal(protoKeys.length,12);
for(const key of protoKeys) for(const suffix of ['', '.BTCUSDT']) {
 const v=context.venue,stats=[],calls=[];let error;
 v.isDisposed=false;v.messageHandlers={};v.recordMessageHandlerStats=k=>stats.push(k);
 v.trackLastReceived=()=>calls.push('received');v.directFallback={onMessage:()=>calls.push('fallback')};
 v.emitOhlcvDiagnosticReceive=()=>calls.push('diagnostic');
 const member=v.messageHandlers[key];const arity=member.length;
 const data=JSON.stringify({topic:key+suffix});
 try{v.onMessage({data})}catch(e){error={name:e.name,message:e.message}}
 const positive=typeof member==='function'&&arity>0;
 assert.deepEqual(calls,['received','fallback','diagnostic']);
 assert.deepEqual(stats,positive?[key]:[],`prototype stats ${key}${suffix}`);
 if(positive){assert.equal(error?.name,'TypeError');assert.match(error.message,/not a function/)}else assert.equal(error,undefined);
 prototypeDispatch.push({key,data,kind:typeof member,arity:arity??null,positive,stats:[...stats],calls:[...calls],error});
}
assert.equal(prototypeDispatch.filter(c=>c.positive).length,16);
assert.equal(prototypeDispatch.filter(c=>c.kind==='function'&&!c.positive).length,6);
assert.equal(prototypeDispatch.filter(c=>c.kind==='object').length,2);

// Pin source mutation semantics independently of the Rust assertions.
{
 const v=context.venue, trace=[];
 v.messageHandlers={tickers:[]};const list=v.messageHandlers.tickers;
 list.push(()=>{trace.push('first');list.splice(0,1)},()=>trace.push('second'),()=>trace.push('third'));
 v.onMessage({data:'{"topic":"tickers.BTCUSDT"}'});
 assert.deepEqual(trace,['first','third']);
 const captured=[], exact=[];const prefix=[()=>captured.push('prefix')];
 exact.push(()=>{captured.push('exact');delete v.messageHandlers.tickers;exact.push(()=>captured.push('appended'));});
 v.messageHandlers={'tickers.BTCUSDT':exact,tickers:prefix};v.onMessage({data:'{"topic":"tickers.BTCUSDT"}'});
 assert.deepEqual(captured,['exact','appended','prefix']);
 assert.equal(Object.is(JSON.parse('{"n":-0}').n,-0),true);
 assert.equal(JSON.parse('{"n":1e400}').n,Infinity);
}
process.stdout.write(JSON.stringify({dispatch,subscriptions,sends,diagnostics,divergences,prototypeDispatch}));
