// Source-only witnesses for held dispatch and inherited handler-statistics seams.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
function binding(ast,name){const stmt=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(stmt,name);return stmt.getText(ast).replace(/^export /,'');}
const base=parse('packages/safe-cex/src/exchanges/base.ws.ts'),klass=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');assert.ok(klass);
const names=['messageHandlers','messageHandlersStats','lastStatsReconcileAt','recordMessageHandlerStats','reconcileMessageHandlerStats'];
let members=names.map(name=>{const m=klass.members.find(n=>n.name?.getText(base)===name);assert.ok(m,name);return m.getText(base);}).join('\n');
if(process.argv.includes('--skip-reconcile'))members=members.replace('this.reconcileMessageHandlerStats(now);','');
if(process.argv.includes('--stats-own-only'))members=members.replace('messageHandlersStats: MessageHandlersStats = {};','messageHandlersStats: MessageHandlersStats = Object.create(null);');
let dispatch=binding(parse('packages/safe-cex/src/exchanges/binance/binance.ws-public.ts'),'dispatchBinanceMessage');
if(process.argv.includes('--own-only'))dispatch=dispatch.replace('messageHandlers[eventType]','Object.hasOwn(messageHandlers,eventType) ? messageHandlers[eventType] : undefined');
if(process.argv.includes('--arity-as-truthiness'))dispatch=dispatch.replaceAll('broadcast?.length','broadcast');
const source=`${binding(parse('packages/json-parse/src/jsonParse.ts'),'jsonParse')}\n${binding(base,'STATS_RECONCILE_INTERVAL_MS')}\n${binding(base,'STATS_RECONCILE_FORCE_MIN_MS')}\nclass StatsFixture {${members}}\n${dispatch}`;
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
const namesAndLengths=[['constructor',1],['__defineGetter__',2],['__defineSetter__',2],['hasOwnProperty',1],['__lookupGetter__',1],['__lookupSetter__',1],['isPrototypeOf',1],['propertyIsEnumerable',1],['toString',0],['valueOf',0],['__proto__',null],['toLocaleString',0]];
const cases=[];
for(const [key,length] of namesAndLengths)for(const exact of [false,true])for(const prefix of [false,true])for(const malformed of [false,true])for(const time of [1000,6000]){
 // A fresh realm prevents source writes onto Object.prototype/native functions
 // from contaminating the next case or the oracle host itself.
 const context=vm.createContext({});
 vm.runInContext(code,context);
 const fixture={key,exact,prefix,malformed,time};
 const result=vm.runInContext(`(() => {
  const input=${JSON.stringify(fixture)},fixture=new StatsFixture(),trace=[];
  Date.now=()=>input.time;
  if(input.exact)fixture.messageHandlers[input.key+'.BTC']=[events=>trace.push({handler:'exact',events})];
  if(input.prefix)fixture.messageHandlers[input.key+'.1m.ETH']=[events=>trace.push({handler:'prefix',events})];
  const frame=JSON.stringify({e:input.key,s:'BTC'}),raw=input.malformed?frame.slice(0,-1):frame;
  let error=null;
  try { dispatchBinanceMessage(fixture.messageHandlers,raw,topic=>{trace.push({stats:topic});fixture.recordMessageHandlerStats(topic);}); }
  catch(e){error={name:e.name,message:e.message};}
  const hit=fixture.messageHandlers[input.key],stat=fixture.messageHandlersStats[input.key];
  return JSON.stringify({hitType:typeof hit,length:hit?.length??null,trace,error,ownStats:Object.keys(fixture.messageHandlersStats),statsJson:JSON.stringify(fixture.messageHandlersStats),statType:typeof stat,prototypeWrites:Object.keys(Object.prototype),hitWrites:Object.keys(hit),lastCallbacksCount:stat?.lastCallbacksCount??null,hadHandlers:stat?.hadHandlers??null,lastMessageTimestamp:stat?.lastMessageTimestamp??null});
 })()`,context);
 const actual=JSON.parse(result),label=`dispatch ${key} exact=${exact} prefix=${prefix} malformed=${malformed} time=${time}`;
 assert.equal(actual.length,length,`${label}: native inherited arity`);
 const selected=Boolean(length)||exact||prefix;
 assert.equal(actual.trace.some(x=>'stats' in x),selected,`${label}: stats before parse`);
 const reconcileFailure=selected&&key==='__proto__'&&time>=5000;
 assert.equal(actual.error?.name??null,reconcileFailure||(selected&&!malformed&&Boolean(length))?'TypeError':null,`${label}: inherited iteration failure`);
 if(reconcileFailure)assert.equal(actual.trace.filter(x=>'handler' in x).length,0,`${label}: reconcile fails before own handler even for malformed input`);
 assert.deepEqual(actual.ownStats,[],`${label}: inherited stats entry stays inherited`);
 assert.equal(actual.statsJson,'{}',`${label}: inherited stats serialization`);
 if(selected){
  assert.ok(actual.hitWrites.includes('lastMessageTimestamp'),`${label}: original inherited member mutated`);
  assert.equal(actual.prototypeWrites.includes('lastMessageTimestamp'),key==='__proto__',`${label}: prototype-object mutation is separate from callable mutation`);
  assert.equal(actual.lastMessageTimestamp,time,`${label}: inherited stats mutation`);
  assert.equal(actual.lastCallbacksCount,length,`${label}: callbacksCount uses inherited arity`);
  assert.equal(actual.hadHandlers,Boolean(length)?true:null,`${label}: hasHandlers uses arity`);
 }
 if(!length&&!malformed&&selected&&!reconcileFailure)assert.equal(actual.trace.at(-1).handler,exact?'exact':'prefix',`${label}: own handler runs`);
 cases.push({...fixture,...actual});
}
assert.equal(cases.length,192);
process.stdout.write(JSON.stringify(cases));
