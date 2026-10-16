import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
function binding(ast,name){const stmt=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(stmt,name);return stmt.getText(ast).replace(/^export /,'');}
const base=parse('packages/safe-cex/src/exchanges/base.ws.ts');
const baseClass=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');
const members=['messageHandlers','messageHandlersStats','lastStatsReconcileAt','recordMessageHandlerStats','reconcileMessageHandlerStats'].map(name=>{const m=baseClass.members.find(n=>n.name?.getText(base)===name);assert.ok(m,name);return m.getText(base);}).join('\n');
const inherited=Object.getOwnPropertyNames(Object.prototype);
assert.equal(inherited.length,12,'all Object.prototype own names');
const topics=['book.BTC','publicTrade.SOLUSDT','publicTrade.BTCUSDT','publicTrade.ETHUSDT',...inherited];
const cases=[];
for(const [variant,className] of [['binance','BinanceMarketWebsocket'],['binance','BinancePublicWebsocket'],['binancepm','BinancePMMarketWebsocket'],['binancepm','BinancePMPublicWebsocket'],['binancespot','BinancePublicWebsocket']]){
 const ast=parse(`packages/safe-cex/src/exchanges/binance/${variant}.ws-public.ts`);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===className);
 const member=klass.members.find(n=>n.name?.getText(ast)==='recordHandlerStats');assert.ok(member);
 let wrapper=member.getText(ast);
 if(process.argv.includes('--skip-wrapper'))wrapper=wrapper.replace('this.recordMessageHandlerStats(topic)','undefined');
 const code=ts.transpileModule(`${binding(base,'STATS_RECONCILE_INTERVAL_MS')}\n${binding(base,'STATS_RECONCILE_FORCE_MIN_MS')}\nclass Base {${members}}\nclass Fixture extends Base {${wrapper}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
 for(const topic of topics)for(const count of [null,0,1,2])for(const ownStats of [false,true])for(const time of [1000,6000]){
  const input={topic,count,ownStats,time};
  const context=vm.createContext({});vm.runInContext(code,context);
  const actual=JSON.parse(vm.runInContext(`(()=>{
   const input=${JSON.stringify(input)},fixture=new Fixture();Date.now=()=>input.time;
   if(input.count!==null)Object.defineProperty(fixture.messageHandlers,input.topic,{value:Array.from({length:input.count},()=>()=>{}),enumerable:true});
   if(input.ownStats)Object.defineProperty(fixture.messageHandlersStats,input.topic,{value:{lastMessageTimestamp:10,lastCallbacksCount:0,hadHandlers:false},enumerable:true});
   const hit=fixture.messageHandlersStats[input.topic];
   let error=null;
   try{fixture.recordHandlerStats(input.topic);}catch(e){error={name:e.name,message:e.message};}
   const changed=hit!==undefined&&Object.hasOwn(hit,'lastMessageTimestamp')&&hit.lastMessageTimestamp===input.time;
   return JSON.stringify({stats:fixture.messageHandlersStats,error,inheritedType:typeof hit,inheritedChanged:!input.ownStats&&changed,prototypeChanged:Object.hasOwn(Object.prototype,'lastMessageTimestamp')});
  })()`,context));
  const skipped=topic==='publicTrade.SOLUSDT';
  if(!skipped&&(!inherited.includes(topic)||ownStats)){
   assert.ok(Object.hasOwn(actual.stats,topic),'wrapper reaches actual core stats');
   assert.equal(actual.stats[topic].lastMessageTimestamp,time,'wrapper reaches actual core stats');
  }
  if(!ownStats&&inherited.includes(topic)){
   assert.equal(actual.inheritedType,topic==='__proto__'?'object':'function','inherited member typeof before serialization');
   assert.equal(actual.inheritedChanged,true,'source writes inherited referent');
   assert.equal(actual.prototypeChanged,topic==='__proto__','prototype object mutation is separate');
  }
  cases.push({variant,className,...input,...actual});
 }
}
assert.equal(cases.length,1280,'five wrappers x sixteen topics x four handler counts x two own-stat states x two clocks');
process.stdout.write(JSON.stringify(cases));
