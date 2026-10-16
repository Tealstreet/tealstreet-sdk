// Actual maintained source methods, isolated per scalar and socket wrapper.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const base=parse('packages/safe-cex/src/exchanges/base.ws.ts');
const klass=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');
const binding=name=>{const s=base.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(base)===name));assert.ok(s,name);return s.getText(base).replace(/^export /,'');};
const members=['messageHandlers','messageHandlersStats','lastStatsReconcileAt','recordMessageHandlerStats','reconcileMessageHandlerStats','cleanupMessageHandlerStats'].map(name=>{const m=klass.members.find(n=>n.name?.getText(base)===name);assert.ok(m,name);let text=m.getText(base);if(process.env.SAFE_CEX_SCALAR_ORACLE_NEGATIVE_CONTROL==='truncate'&&name==='recordMessageHandlerStats'){const target='const callbacksCount = handlers ? handlers.length : 1;';assert.ok(text.includes(target),'source scalar corruption target');text=text.replace(target,'const callbacksCount = handlers ? Math.max(0, Math.trunc(handlers.length)) : 1;');}return text;}).join('\n');
const values=['-0.5','0.5','-0','NaN','Infinity','-Infinity','undefined','null','false','true','"0"','""'];
const rows=[];
for(const [variant,className] of [['binance','BinanceMarketWebsocket'],['binance','BinancePublicWebsocket'],['binancepm','BinancePMMarketWebsocket'],['binancepm','BinancePMPublicWebsocket'],['binancespot','BinancePublicWebsocket']]){
 const ast=parse(`packages/safe-cex/src/exchanges/binance/${variant}.ws-public.ts`),c=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===className),wrapper=c.members.find(n=>n.name?.getText(ast)==='recordHandlerStats');assert.ok(wrapper);
 const code=ts.transpileModule(`${binding('STATS_RECONCILE_INTERVAL_MS')}\n${binding('STATS_RECONCILE_FORCE_MIN_MS')}\nclass Base{${members}}\nclass Fixture extends Base{${wrapper.getText(ast)}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 for(const [caseIndex,expression] of values.entries())for(const own of [false,true]){
  const context=vm.createContext({exports:{}});vm.runInContext(code,context);
  const result=vm.runInContext(`(()=>{
   let now=1000;Date.now=()=>now;
   Object.defineProperty(Object.prototype,'length',{value:${expression},writable:true,configurable:true,enumerable:true});
   const socket=new Fixture();
   if(${own})Object.defineProperty(socket.messageHandlersStats,'__proto__',{value:{lastMessageTimestamp:1},writable:true,configurable:true,enumerable:true});
   const encode=value=>{if(typeof value==='number'){const b=new ArrayBuffer(8),v=new DataView(b);v.setFloat64(0,value);return {kind:'number',bits:v.getBigUint64(0).toString(16).padStart(16,'0')};}return {kind:value===null?'null':typeof value,...(value===undefined?{}:{value})};};
   const caught=run=>{try{run();return null;}catch(e){return {name:e.name,message:e.message};}};
   const recordError=caught(()=>socket.recordHandlerStats('__proto__'));
   const entry=socket.messageHandlersStats.__proto__;
   const count=encode(entry.lastCallbacksCount),had=Boolean(entry.hadHandlers);
   const hadOwn=Object.hasOwn(entry,'hadHandlers'),hadValue=encode(entry.hadHandlers);
   const jsonView=JSON.parse(JSON.stringify({lastCallbacksCount:entry.lastCallbacksCount}));
   now=6000;const laterError=caught(()=>socket.recordHandlerStats('ordinary'));
   const ordinaryTime=socket.messageHandlersStats.ordinary.lastMessageTimestamp;
   now=6001;const retryError=caught(()=>socket.recordHandlerStats('ordinary'));
   now=6500;const cleanupError=caught(()=>socket.cleanupMessageHandlerStats('ordinary'));
   const ordinaryDeleted=!Object.hasOwn(socket.messageHandlersStats,'ordinary');
   return {count,had,hadOwn,hadValue,jsonView,recordError,laterError,ordinaryTime,retryError,cleanupError,ordinaryDeleted};
  })()`,context);
  rows.push({variant,className,caseIndex,own,...result});
 }
}
assert.equal(rows.length,120,'five source wrappers by twelve scalars by two own-record layouts');
process.stdout.write(JSON.stringify(rows));
