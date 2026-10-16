import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
function binding(ast,name){const stmt=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(stmt,name);return stmt.getText(ast).replace(/^export /,'');}
const base=parse('packages/safe-cex/src/exchanges/base.ws.ts'),baseClass=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');
const members=['messageHandlers','messageHandlersStats','lastStatsReconcileAt','recordMessageHandlerStats','reconcileMessageHandlerStats','cleanupMessageHandlerStats'].map(name=>{const m=baseClass.members.find(n=>n.name?.getText(base)===name);assert.ok(m,name);let text=m.getText(base);if(process.argv.includes('--wrong-inherited-write')&&name==='recordMessageHandlerStats'){assert.ok(text.includes('existing.lastMessageTimestamp = now;'),'source write mutation target');text=text.replace('existing.lastMessageTimestamp = now;','existing.lastMessageTimestamp = now + 1;');}return text;}).join('\n');
const enums={};const types=parse('packages/safe-cex/src/types.ts');
for(const name of ['OrderSide','OrderStatus','OrderType','PositionSide']){const declaration=types.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text===name);assert.ok(declaration);vm.runInNewContext(ts.transpileModule(declaration.getText(types),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});}
const history=fs.readFileSync(new URL('packages/safe-cex/src/exchanges/binance/binance.order-history.ts',root),'utf8');
const cases=[];
for(const [variant,className] of [['binance','BinanceMarketWebsocket'],['binance','BinancePublicWebsocket'],['binancepm','BinancePMMarketWebsocket'],['binancepm','BinancePMPublicWebsocket'],['binancespot','BinancePublicWebsocket']]){
 const ast=parse(`packages/safe-cex/src/exchanges/binance/${variant}.ws-public.ts`),klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===className),wrapper=klass.members.find(n=>n.name?.getText(ast)==='recordHandlerStats');assert.ok(wrapper);
 const code=ts.transpileModule(`${binding(base,'STATS_RECONCILE_INTERVAL_MS')}\n${binding(base,'STATS_RECONCILE_FORCE_MIN_MS')}\nclass Base{${members}}\nclass Fixture extends Base{${wrapper.getText(ast)}}\n${history}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
 const fresh=()=>{const context=vm.createContext({exports:{},require:id=>{assert.equal(id,'../../types');return enums;}});vm.runInContext(code,context);return context;};
 for(const count of [null,0,1,2]){
  const context=fresh();
  const output=vm.runInContext(`(()=>{
   const a=new Fixture(),b=new Fixture(),count=${JSON.stringify(count)};
   const fields=value=>Object.fromEntries(Object.entries(value).map(([key,value])=>[key,value===undefined?'[undefined]':value]));
   let now=1000;Date.now=()=>now;
   if(count!==null)Object.defineProperty(a.messageHandlers,'__proto__',{value:Array.from({length:count},()=>()=>{}),enumerable:true});
   a.recordHandlerStats('__proto__');
   if(Object.prototype.lastMessageTimestamp!==1000)throw Error('source writes shared prototype timestamp');
   if(Object.prototype.lastCallbacksCount!==count&&(count!==null||Object.prototype.lastCallbacksCount!==undefined))throw Error('source shared callback count');
   const initial=fields(Object.prototype);
   now=1010;a.recordHandlerStats('constructor');
   const savedConstructor=a.messageHandlersStats.constructor;
   if(savedConstructor!==b.messageHandlersStats.constructor)throw Error('source shares constructor referent between sockets');
   if(savedConstructor.prototype!==Object.prototype)throw Error('source constructor prototype aliases mutated object');
   if(({}).lastMessageTimestamp!==1000)throw Error('source fresh ordinary object observes prototype mutation');
   const row={orderId:'12',symbol:'BTCUSDT',time:1,side:'BUY',origType:'lastMessageTimestamp',type:'MARKET',status:'lastCallbacksCount',positionSide:'hadHandlers'};
   const historical=exports.mapBinanceHistoricalOrder(row);
   if(historical.type!==1000)throw Error('source historical numeric inherited value suppresses fallback');
   const fallback=exports.mapBinanceHistoricalOrder({...row,origType:'lastCallbacksCount'});
   if(fallback.type!==(count===null?'market':count))throw Error('source historical nullish and zero fallback differ');
   const functionOutput=exports.mapBinanceHistoricalOrder({...row,origType:'constructor'});
   if(functionOutput.type!==savedConstructor||functionOutput.type.lastMessageTimestamp!==1010)throw Error('source historical function retains mutated referent identity');
   const prototypeOutput=exports.mapBinanceHistoricalOrder({...row,origType:'__proto__'});
   if(prototypeOutput.type!==Object.prototype)throw Error('source historical prototype output retains identity');
   const caught=run=>{try{run();return null;}catch(e){return{name:e.name,message:e.message};}};
   now=6000;const ordinaryError=caught(()=>b.recordHandlerStats('book.BTC'));
   if(Boolean(ordinaryError)!==(count!==0))throw Error('source later ordinary topic reconciles polluted prototype');
   if(b.messageHandlersStats['book.BTC'].lastMessageTimestamp!==6000)throw Error('source ordinary stat commits before throw');
   now=6001;const retryError=caught(()=>b.recordHandlerStats('book.BTC'));
   if(retryError!==null)throw Error('source failed reconciliation commits throttle before throw');
   now=6249;const cleanupEarlyError=caught(()=>b.cleanupMessageHandlerStats('book.BTC'));
   if(cleanupEarlyError!==null||Object.hasOwn(b.messageHandlersStats,'book.BTC'))throw Error('source early cleanup deletes before force throttle');
   now=6500;b.recordHandlerStats('book.BTC');const cleanupError=caught(()=>b.cleanupMessageHandlerStats('book.BTC'));
   if(Boolean(cleanupError)!==(count!==0)||Object.hasOwn(b.messageHandlersStats,'book.BTC'))throw Error('source cleanup deletes before inherited failure');
   now=7000;const partial={};
   for(const ownUndefined of [false,true]){
    const probe=new Fixture();probe.messageHandlersStats.partial={lastMessageTimestamp:1,lastCallbacksCount:9};
    if(ownUndefined)Object.defineProperty(probe.messageHandlersStats.partial,'hadHandlers',{value:undefined,enumerable:true});
    const error=caught(()=>probe.recordHandlerStats('probe.BTC'));
    const actual=probe.messageHandlersStats.partial;
    const expected=ownUndefined||!count?9:0;
    if(actual.lastCallbacksCount!==expected)throw Error('source own undefined shadows inherited hadHandlers');
    partial[ownUndefined?'explicitUndefined':'absent']={count:actual.lastCallbacksCount,ownUndefined:Object.hasOwn(actual,'hadHandlers'),error};
   }
   globalThis.savedConstructor=savedConstructor;
   globalThis.savedMapper=()=>exports.mapBinanceHistoricalOrder(row);
   return JSON.stringify({initial,constructorFields:fields(savedConstructor),historical:{type:historical.type,status:historical.status,positionSide:historical.positionSide},fallbackType:fallback.type,prototypeFields:fields(prototypeOutput.type),ordinaryError,retryError,cleanupEarlyError,cleanupError,afterCleanupKeys:Object.keys(b.messageHandlersStats),partial,lastReconcile:b.lastStatsReconcileAt});
  })()`,context);
  const other=fresh();vm.runInContext(`const other=new Fixture();Date.now=()=>2000;other.recordHandlerStats('constructor');globalThis.otherConstructor=other.messageHandlersStats.constructor;`,other);
  assert.notEqual(context.savedConstructor,other.otherConstructor,'different realms have distinct constructor identity');
  assert.equal(context.savedConstructor.lastMessageTimestamp,1010,'saved constructor retains original realm');
  assert.equal(other.otherConstructor.lastMessageTimestamp,2000,'second realm has independent constructor writes');
  assert.equal(context.savedMapper().type,1000,'saved source function executes in original module realm');
  cases.push({variant,className,count,...JSON.parse(output),realms:{firstTimestamp:context.savedConstructor.lastMessageTimestamp,secondTimestamp:other.otherConstructor.lastMessageTimestamp,distinct:true,savedMapperType:context.savedMapper().type}});
 }
}
assert.equal(cases.length,20,'independent five wrappers by four handler counts');process.stdout.write(JSON.stringify(cases));
