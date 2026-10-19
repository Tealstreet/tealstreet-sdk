import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash} from 'node:crypto';
import {root,ts,load} from '../tests/source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts';
const source=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPublicWebsocket');
assert.ok(klass,'maintained public class');
const key=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='subscriptionKey'));
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const parser=load('packages/json-parse/src/jsonParse.ts');
// Only inherited transport, clock and diagnostics terminate here. Every public
// class member, constructor registration and Store mutation runs maintained code.
class BaseWebSocket {
 constructor(parent){this.parent=parent;this.store=parent.store;this.isDisposed=false;this.isConnected=false;}
 send(data){this.parent.sent.push(JSON.parse(data));this.parent.sendHook?.(JSON.parse(data));}
 emitWsDiagnosticSend(){} emitWsDiagnosticReceive(){} emitWsReconnected(){}
 trackLastReceived(){} handlePongEvent(){} recordMessageHandlerStats(){} cleanupMessageHandlerStats(){}
 dispose(){this.isDisposed=true;}
}
const context={exports:{},...utils,...types,...parser,BaseWebSocket,performance:{now:()=>1250},
 LogSeverity:{Warning:'warning'},setInterval:()=>1,clearInterval(){}};
vm.runInNewContext(ts.transpileModule(key.getText(ast)+'\n'+klass.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
function construct(){
 const listeners=new Map(),store=new DefaultStore();store.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:.5,maxLeverage:20})]});
 const parent={store,sent:[],on(name,fn){listeners.set(name,fn);},off(name,fn){assert.equal(listeners.get(name),fn);listeners.delete(name);},getActiveTickerSymbols(){return this.active??[];}};
 const socket=new context.exports.MexcPublicWebsocket(parent);return {socket,parent,listeners};
}
const options=interval=>({symbol:'BTCUSDT',interval});
const frame={data:JSON.stringify({channel:'push.kline',symbol:'BTC_USDT',data:{interval:'Min1',time:1700000000,open:11,high:13,low:7,close:12,vol:5}})};
const {socket,parent,listeners}=construct();const trace=[];let later,turn=0;
const cleanup=socket.listenOHLCV(options('1m'),()=>{trace.push('A');if(turn++===0)later=socket.listenOHLCV(options('1m'),()=>trace.push('B'));else if(turn===2)later();});
for(let i=0;i<3;i++)socket.onMessage(frame);
assert.deepEqual(trace,['A','A','B','A'],'maintained captured-array add/remove');cleanup();socket.onMessage(frame);assert.equal(trace.length,4);
const second=construct();let added;
const first=second.socket.listenOHLCV(options('1m'),()=>{});
second.parent.sendHook=frame=>{if(frame.method==='sub.kline'&&frame.param.interval==='Min1'&&!added)added=second.socket.listenOHLCV(options('5m'),()=>{});};
second.socket.isConnected=true;second.socket.onOpen();
const methods=second.parent.sent.map(frame=>`${frame.method}:${frame.param?.interval??''}`);
assert.deepEqual(methods,['ping:','sub.ticker:','sub.kline:Min1','sub.kline:Min5','sub.kline:Min5','sub.ticker:'],'maintained live resubscribe insertion order');
first();added();second.parent.active=['missing'];second.listeners.get('activeTickerSymbols:changed')();
assert.deepEqual(second.parent.sent.at(-1),{method:'unsub.ticker',param:{symbol:'BTC_USDT'}});
second.socket.dispose();assert.equal(second.listeners.size,0,'maintained constructor listener detaches');
socket.dispose();assert.equal(listeners.size,0);
process.stdout.write(JSON.stringify({sha256:createHash('sha256').update(source).digest('hex'),trace,methods,detached:true}));
