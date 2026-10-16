import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
function code(file,member){
 const url=new URL('../../../packages/safe-cex/src/exchanges/okx/'+file,import.meta.url);
 const ast=ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(ts.isClassDeclaration),method=cls.members.find(n=>n.name?.getText(ast)===member);
 assert.ok(method,'maintained '+file+'/'+member);
 return ts.transpileModule('export class Selected {'+method.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
}
const codes={open:code('okx.ws-public.ts','onOpen'),business:code('okx.ws-business.ts','subscribe'),private:code('okx.ws-private.ts','subscribe'),notional:code('okx.exchange.ts','contractsToNotional')};
function owner(source,sandbox={}){const module={exports:{}};vm.runInNewContext(source,{...sandbox,module,exports:module.exports});return new module.exports.Selected();}
const input=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(input.open.length,48);assert.equal(input.subscriptions.length,240);assert.equal(input.notional.length,60);
const text=v=>String.fromCharCode(...v),units=v=>Array.from({length:v.length},(_,i)=>v.charCodeAt(i));
const pack=v=>{if(Number.isNaN(v))return {nan:true};const b=Buffer.alloc(8);b.writeDoubleBE(v);return {bits:b.toString('hex')};};
const decode=v=>v===null?undefined:Buffer.from(v,'hex').readDoubleBE();
const open=[];
for(const c of input.open){
 const events=[],failure=new TypeError('public callback failure');let enabled=c.enabled,disposed=c.disposed,index=0;
 const sandbox={module:{exports:{}}};sandbox.exports=sandbox.module.exports;
 Object.defineProperty(sandbox,'ENABLE_RECONNECTING_WEBSOCKET',{get(){events.push('enabled');return enabled;}});
 vm.runInNewContext(codes.open,sandbox);const receiver=new sandbox.module.exports.Selected();
 Object.defineProperty(receiver,'isDisposed',{get:()=>disposed});
 const invoke=action=>{events.push(action);const i=index++;if(i===c.flip){disposed=true;enabled=!enabled;}if(i===c.failAt)throw failure;};
 receiver.orderBookSequenceByHandler={clear:()=>invoke('orderBookSequenceByHandler.clear')};receiver.recoveringOrderBooks={clear:()=>invoke('recoveringOrderBooks.clear')};
 receiver.subscribe=()=>invoke('subscribe');receiver.ping=()=>invoke('ping');receiver.onMarketsChanged=()=>{};
 receiver.parent={emitter:{off:(event,handler)=>{assert.equal(event,'markets:changed');assert.equal(handler,receiver.onMarketsChanged);invoke('emitter.off:'+event);receiver.onMarketsChanged=()=>{};},on:(event,handler)=>{assert.equal(event,'markets:changed');assert.equal(handler,receiver.onMarketsChanged);invoke('emitter.on:'+event);}}};
 receiver.emitWsReconnected=()=>invoke('emitWsReconnected');let error=null;
 try{receiver.onOpen();}catch(e){assert.equal(e,failure);error={name:e.name,message:e.message,same:true};}
 open.push({events,error});
}
const subscriptions=[];
for(const c of input.subscriptions){
 const receiver=owner(c.business?codes.business:codes.private,{flatten:arrays=>arrays.flat()}),trace=[],frames=[],failure=new TypeError('public callback failure');let sendIndex=0,snapshot;
 receiver.parent={getInstrumentTypes:()=>{trace.push('instruments');if(c.failAt===-2)throw failure;return c.instruments;}};
 let topics={};Object.defineProperty(receiver,'topics',{get(){trace.push('topics');if(c.failAt===-1)throw failure;snapshot=topics.all;return topics;}});
 receiver.sendWithLog=wire=>{
  const i=sendIndex++;trace.push('send:'+i);const frame=JSON.parse(wire);
  if(i>0)for(const topic of frame.args)topic.instId=units(topic.instId);
  frames.push(frame);if(i===c.failAt)throw failure;
  if(c.business&&i===0)topics={all:Array.from({length:c.count},(_,j)=>({channel:'books',instId:'S'+j}))};
  if(c.business&&i===1&&c.count>100)snapshot[c.count-1].instId=text([0xd800,77]);
 };let error=null;
 try{receiver.subscribe();}catch(e){assert.equal(e,failure);error={name:e.name,message:e.message,same:true};}
 subscriptions.push({trace,frames,error});
}
const notional=[];
for(const c of input.notional){
 const receiver=owner(codes.notional),trace=[],failure=new TypeError('public callback failure');
 const field=(i,name)=>{trace.push(name);if(i===c.failAt)throw failure;};
 const market={get contractSize(){field(0,'size');return decode(c.size);},get isInverse(){field(2,'inverse');return c.inverse;},get quote(){field(3,'quote');return text(c.quote);}};
 receiver.getCtValCcy=()=>{field(1,'ccy');return text(c.ccy);};let outcome;
 try{outcome=pack(receiver.contractsToNotional(decode(c.contracts),market,decode(c.mark)));}catch(e){assert.equal(e,failure);outcome={error:{name:e.name,message:e.message,same:true}};}
 notional.push({trace,outcome});
}
process.stdout.write(JSON.stringify({open,subscriptions,notional}));
