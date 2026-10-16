import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const ts=createRequire(import.meta.url)(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const lanes=[['futures_market_message','binance','BinanceMarketWebsocket'],['futures_public_message','binance','BinancePublicWebsocket'],['pm_market_message','binancepm','BinancePMMarketWebsocket'],['pm_public_message','binancepm','BinancePMPublicWebsocket'],['spot_public_message','binancespot','BinancePublicWebsocket'],['futures_private_message','binance','BinancePrivateWebsocket'],['pm_private_message','binancepm','BinancePMPrivateWebsocket']];
const publicCases=['normal','disposed','read-fail','read-fail-disposed','read-disposes','read-revives','health-fail','health-disposes','repeat','dispatch-fail','missing-data','object-data','dispatch-mutates','preinitialized'];
const privateCases=['account','order','special','pong','account-pong','order-pong','special-pong','id-string','unknown','invalid','null','array','primitive','big-id','string-corruption','object-data','array-data','coercion-fail','disposed','read-fail','health-fail','health-disposes','diagnostic-fail','diagnostic-disposes','diagnostic-mutates','account-fail','order-fail','special-fail','pong-fail','account-mutates','order-mutates','callback-disposes','missing-data','read-disposes','read-revives','preinitialized'];
const delegateCases=['normal','listen-fail','callback-fail','cleanup-fail','mutate-options','repeat-cleanup'];
function extract(venue,klass,member,kind='public'){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${venue}.ws-${kind}.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass),m=decl?.members.find(n=>n.name?.getText(ast)===member);assert.ok(m);return m.getText(ast);
}
const baseFile=new URL('../../../packages/safe-cex/src/exchanges/base.ws.ts',import.meta.url),baseAst=ts.createSourceFile(baseFile.pathname,fs.readFileSync(baseFile,'utf8'),ts.ScriptTarget.Latest,true),baseDecl=baseAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');
const baseMembers=['isDisposed','trackLastReceived','recordMessageHandlerStats','reconcileMessageHandlerStats'].map(name=>{const m=baseDecl.members.find(n=>n.name?.getText(baseAst)===name);assert.ok(m);return m.getText(baseAst);});
const parserExports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL('../../../packages/json-parse/src/jsonParse.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:parserExports});
const baseConstants=baseAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>['STATS_RECONCILE_INTERVAL_MS','STATS_RECONCILE_FORCE_MIN_MS'].includes(d.name.getText(baseAst)))).map(n=>n.getText(baseAst)).join('\n');assert.equal(baseConstants.split('const ').length-1,2);
let active;
const compile=body=>{const exports={};vm.runInNewContext(ts.transpileModule(`${baseConstants}\nexports.Subject=class {${body}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,...parserExports,Date:{now:()=>{active.trace.push('clock');return 1250;}},setTimeout:()=>{active.timers++;return {id:1};},clearTimeout:()=>{},dispatchBinanceMessage:(handlers,data,stats)=>active.dispatch(handlers,data,stats)});return exports.Subject;};
function privateInput(name,venue){
 const special=venue==='binance'?'ALGO_UPDATE':'CONDITIONAL_ORDER_TRADE_UPDATE';
 let e=name.startsWith('order')?'ORDER_TRADE_UPDATE':name.startsWith('special')?special:'ACCOUNT_UPDATE';
 if(['pong','pong-fail','unknown','id-string'].includes(name))e='OTHER';
 const obj={e,o:{i:'small-id',q:'3.6',p:'12.3'},id:(name.includes('pong')||name.endsWith('mutates'))?42:7};
 if(name==='id-string')obj.id='42';
 if(name==='object-data'||name==='diagnostic-mutates')return obj;
 if(name==='array-data')return [JSON.stringify(obj)];
 if(name==='coercion-fail')return {toString:0};
 if(name==='big-id')return '{"e":"ORDER_TRADE_UPDATE","o":{"i":9007199254740993,"q":"3.6","p":"12.3"},"id":7}';
 if(name==='string-corruption')return '{"e":"ORDER_TRADE_UPDATE","o":{"i":"x:1234567890123456,","q":"3.6","p":"12.3"}}';
 if(name==='invalid')return 'not-json';if(name==='null')return 'null';if(name==='array')return '[]';if(name==='primitive')return '1';if(name==='missing-data')return undefined;
 return JSON.stringify(obj);
}
const results=[];
for(const [lane,venue,klass]of lanes){
 const priv=lane.includes('private'),Subject=compile([...baseMembers,extract(venue,klass,'onMessage',priv?'private':'public')].join('\n'));
 for(const name of priv?privateCases:publicCases){
  const trace=[],calls=[],error=new RangeError('message fixture'),instance=new Subject();let timers=0,health=0,reads=0,dispatched=0,failure=null,errorName=null,errorMessage=null;
  let data=priv?privateInput(name,venue):name==='missing-data'?undefined:name==='object-data'||name==='dispatch-mutates'?{e:'bookTicker',q:'3.6',p:'12.3'}:'{"e":"bookTicker","q":"3.6","p":"12.3"}';
  const stats=()=>{};const handlers={marker:3.6};
  instance.parent={isDisposed:['disposed','read-fail-disposed','read-revives'].includes(name),syncSocketHealthToStore(){trace.push('health');health++;if(name==='health-disposes')this.isDisposed=true;if(name==='health-fail')throw error;}};
  Object.assign(instance,{lastReceived:0,isInitialized:false,onCloseCount:0,reconnectCount:0,reconnectBackoffAttempts:0,reconnectBackoffLastFailureAt:0,consecutiveCloseFailures:0,reconnectPauseUntil:0,treatAnyMessageAsPong:false,messageHandlers:handlers,messageHandlersStats:{},lastStatsReconcile:0});
  instance.recordHandlerStats=stats;
  const event={get data(){trace.push('read');reads++;if(name.startsWith('read-fail'))throw error;if(name==='read-disposes')instance.parent.isDisposed=true;if(name==='read-revives')instance.parent.isDisposed=false;return data;}};
  active={trace,timers:0,dispatch(h,d,s){trace.push('dispatch');dispatched++;assert.equal(h,handlers,'original handler dictionary');assert.equal(s,stats,'original stats callback');assert.equal(d,data,'original data referent');if(name==='dispatch-mutates')d.q='4';if(name==='dispatch-fail')throw error;}};
  instance.emitWsDiagnosticReceive=d=>{trace.push('diagnostic');assert.equal(d,data);if(name==='diagnostic-disposes')instance.parent.isDisposed=true;if(name==='diagnostic-mutates')d.e='ORDER_TRADE_UPDATE';if(name==='diagnostic-fail')throw error;};
  let previous,previousArray;
  const handle=kind=>events=>{trace.push(kind);assert.equal(events.length,1,'one native payload');const json=events[0];if(previous){assert.equal(json,previous,'original JSON referent');assert.notEqual(events,previousArray,'fresh singleton argument');}previous=json;previousArray=events;calls.push({kind,payload:structuredClone(json),identity:typeof data==='object'&&data!==null&&!Array.isArray(data)?json===data:true});if(name===kind+'-fail'||name==='special-fail'&&['algo','conditional'].includes(kind))throw error;if(name==='account-mutates'&&kind==='account'){json.e='ORDER_TRADE_UPDATE';json.o.q='4';}if(name==='order-mutates'&&kind==='order'){json.e=venue==='binance'?'ALGO_UPDATE':'CONDITIONAL_ORDER_TRADE_UPDATE';json.o.p='14.2';}if(name==='callback-disposes')instance.parent.isDisposed=true;};
  instance.handleAccountEvents=handle('account');instance.handleOrderEvents=handle('order');instance.handleAlgoOrderEvents=handle('algo');instance.handleConditionalOrderEvents=handle('conditional');
  instance.handlePongEvent=()=>{trace.push('pong');calls.push({kind:'pong'});if(name==='pong-fail')throw error;};
  if(name==='preinitialized'){instance.trackLastReceived();trace.length=0;health=0;active.timers=0;}
  try{instance.onMessage(event);if(name==='repeat')instance.onMessage(event);}catch(e){failure=e===error?'original':'generated';errorName=e.name;errorMessage=e.message;if(e!==error)assert.equal(e.name,'TypeError','unexpected source extraction error');}
  const topics=Object.keys(instance.messageHandlersStats).sort(),statRows=topics.map(topic=>[topic,instance.messageHandlersStats[topic].lastMessageTimestamp,instance.messageHandlersStats[topic].lastCallbacksCount,instance.messageHandlersStats[topic].hadHandlers]);
  results.push({lane,name,trace,calls,reads,dispatched,health,timers:active.timers,disposed:instance.parent.isDisposed,initialized:instance.isInitialized,last:instance.lastReceived||null,failure,errorName,errorMessage,stats:statRows,data:data===undefined?null:data});
 }
}
for(const[lane,venue,klass]of [['futures_candle_delegate','binance','BinancePublicWebsocket'],['pm_candle_delegate','binancepm','BinancePMPublicWebsocket']]){
 const Subject=compile(extract(venue,klass,'listenOHLCV'));
 for(const name of delegateCases){
  const instance=new Subject(),trace=[],opts={symbol:'BTCUSDT',interval:'1m',from:100,to:200,limit:7},candle={timestamp:1000,open:11,high:22,low:3,close:12.3,volume:3.6},error=new RangeError('delegate fixture');let listens=0,callbacks=0,cleanups=0,failure=null,cleanupFailure=null;
  const callback=c=>{trace.push('callback');callbacks++;assert.equal(c,candle);c.close=20;if(name==='callback-fail')throw error;};
  const cleanup=()=>{trace.push('cleanup');cleanups++;if(name==='cleanup-fail')throw error;};
  instance.marketWebsocket={listenOHLCV(o,c){trace.push('listen');listens++;assert.equal(o,opts);assert.equal(c,callback);if(name==='mutate-options')o.limit=9;if(name==='listen-fail')throw error;c(candle);return cleanup;}};
  let returned;
  try{returned=instance.listenOHLCV(opts,callback);}catch(e){assert.equal(e,error);failure='original';}
  const sameCleanup=returned===cleanup;
  if(returned){try{returned();if(name==='repeat-cleanup')returned();}catch(e){assert.equal(e,error);cleanupFailure='original';}}
  results.push({lane,name,trace,listens,callbacks,cleanups,failure,cleanupFailure,sameCleanup,opts,candle});
 }
}
assert.equal(results.length,154,'independent floor 5*14+2*36+2*6, stopped repeat-cleanup');
process.stdout.write(JSON.stringify(results));
