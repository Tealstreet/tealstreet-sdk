// Reachability witness: venue ping -> real send/reconnect/close -> real Store notification.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function load(file,deps){
 const exports={};
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL(file,root),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:name=>{assert.ok(name in deps,`unexpected import ${name}`);return deps[name];},structuredClone});
 return exports;
}
const typesAst=ts.createSourceFile('types.ts',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enumText=typesAst.statements.filter(n=>ts.isEnumDeclaration(n)).map(n=>n.getText(typesAst)).join('\n');
const types={};vm.runInNewContext(ts.transpileModule(enumText,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:types});
const {DefaultStore}=load('store/store.base.ts',{'lodash/uniqBy':runtimeRequire(process.argv.includes('--missing-runtime-import')?'lodash/uniqBy-deliberately-absent':'lodash/uniqBy'),'../types':types,'../utils/clone':load('utils/clone.ts',{}),'../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1000}}});
const {LogSeverity}=load('../../logging/src/LogSeverity.ts',{});
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const ws=parse('exchanges/base.ws.ts'),exchange=parse('exchanges/base.ts');
function members(ast,names,className){
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&(!className||n.name?.text===className)&&n.members.some(m=>names.includes(m.name?.getText(ast))));assert.ok(klass);
 return names.map(name=>{const m=klass.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,name);return m.getText(ast);}).join('\n');
}
function binding(ast,name){
 const statement=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(statement,name);return statement.getText(ast).replace(/^export /,'');
}
const heartbeat=ws.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text==='isHeartbeatFrame');assert.ok(heartbeat);
const wsMembers=members(ws,['isDisposed','sendWithLog','send','reconnect','onClose','processClose','recordCloseDiagnostics','registerCloseResult','getCloseLogMessage','resetOrderbook']);
const parentMembers=members(exchange,['getPublicSockets','getPrivateSockets','startWatchdog','syncSocketHealthToStore','syncSocketHealthToStoreInternal']);
const results=[];
const entries=[
 ...['binance','binancepm','binancespot'].map(variant=>({variant,scope:'private'})),
 {variant:'binance',scope:'public',className:'BinanceMarketWebsocket',market:true},
 {variant:'binance',scope:'public',className:'BinancePublicWebsocket'},
 {variant:'binancepm',scope:'public',className:'BinancePMMarketWebsocket',market:true},
 {variant:'binancepm',scope:'public',className:'BinancePMPublicWebsocket'},
 {variant:'binancespot',scope:'public',className:'BinancePublicWebsocket'},
];
for(const {variant,scope,className,market=false} of entries){
 const venue=parse(`exchanges/binance/${variant}.ws-${scope}.ts`);
 const spotConstant=scope==='private'&&variant==='binancespot'?binding(venue,'BINANCE_SPOT_HEARTBEAT_REQUEST_ID')+'\n'+binding(venue,'BINANCE_SPOT_USER_DATA_REQUEST_ID'):'';
 let code=`${heartbeat.getText(ws).replace(/^export /,'')}\n${binding(exchange,'summarizeSocketDiagnostics')}\n${spotConstant}\nexports.Socket=class {${wsMembers}\n${members(venue,scope==='public'?['onOpen']:['ping','onOpen'],className)}};\nexports.Parent=class {${parentMembers}};`;
 if(process.argv.includes('--bypass-health-write'))code=code.replace('this.store.updateSocketHealth(', 'void (');
 if(process.argv.includes('--await-spot-reconnect')&&scope==='private'&&variant==='binancespot')code=code.replace('const params = await this.buildUserDataSubscriptionParams();','const params = await this.buildUserDataSubscriptionParams(); await this.reconnect();');
 if(process.argv.includes('--skip-reset'))code=code.replace('this.resetOrderbook();','void 0;');
 if(process.argv.includes('--force-public-send'))code=code.replace("this.parent.options.publicDataOnly ? [] : ['!bookTicker']", "['!bookTicker']");
 if(process.argv.includes('--skip-open-continuation'))code=code.replace('this.hasOpened = true;', 'void 0;');
 if(process.argv.includes('--skip-reconnect'))code=code.replace('this.reconnect();','void 0;');
 const exported={};
 vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{
  exports:exported,LogSeverity,WebSocket:{OPEN:1},ReconnectingWebSocket:class {},
  lifecycleLog(){},performance:{now:()=>123},clearTimeout(){},
 });
 for(const method of scope==='public'?['onOpen']:['ping','onOpen'])for(const publicDataOnly of scope==='public'?[false,true]:[false])for(const disposed of [false,true])for(const listenerThrows of [false,true]){
  const trace=[],store=new DefaultStore(),sentinel=Object.assign(new RangeError('socket health subscriber failed'),{code:'native-health'});
  const parent=Object.assign(new exported.Parent(),{store,isDisposed:disposed,watchdogTimer:1,name:'fixture',options:{publicDataOnly},log(){}});
  const socket=Object.assign(new exported.Socket(),{parent,reconnecting:false,onCloseCount:100,lastReceived:42,subscriptionAttempt:0,buildUserDataSubscriptionParams:async()=>({apiKey:'fixture'}),emitAndLogError(){trace.push('emit-error');},forceClose(){trace.push('force-close');},orderBookInnerState:{fixture:{updates:[1],isSnapshotLoaded:true}},subscribe(){trace.push('subscribe');},resyncOrdersOnOpen:true,hasOpened:false,throttledFetchOrders(){trace.push('fetch-orders');},emitWsDiagnosticSend(){trace.push('diagnostic');}});
  // The source reaches its existing >100 guard after the health publication.
  // This bounds the positive control without substituting a reconnect timer.
  const unsub1=store.subscribe(data=>{trace.push(['first',data.socketHealth.publicHealthy]);if(listenerThrows)throw sentinel;});
  const unsub2=store.subscribe(data=>trace.push(['second',data.socketHealth.publicHealthy]));
  let background=null,settlement=null,observedError=null;
  const reconnect=socket.reconnect;
  socket.reconnect=()=>{
   const promise=reconnect();
   // Observe the exact promise the source send discards; do not await it there.
   background=promise.then(()=>{settlement='resolved';},error=>{assert.equal(error,sentinel,'background rejection preserves subscriber identity');assert.equal(error.name,'RangeError','background rejection preserves native error name');assert.equal(error.code,'native-health','background rejection preserves native error code');observedError=error;settlement='rejected';});
   return promise;
  };
  const returned=socket[method]();trace.push('caller-returned');
  const asyncOpen=scope==='private'&&variant==='binancespot'&&method==='onOpen';
  if(asyncOpen)assert.equal(typeof returned?.then,'function','Spot open returns its own promise');
  else assert.equal(returned,undefined,'venue operation returns synchronously despite background rejection');
  assert.equal(settlement,null,'send does not await reconnect settlement');
  if(asyncOpen){await returned;trace.push('caller-resolved');}
  if(background)await background;
  const sends=!disposed&&(scope==='private'||market||variant==='binancespot'||!publicDataOnly);
  const expected=[...asyncOpen?['caller-returned']:[],...sends?[['first',true],...listenerThrows?[]:[['second',true]]]:[],...!disposed&&scope==='private'?['diagnostic']:[],...!disposed&&scope==='private'&&method==='onOpen'&&!asyncOpen?['fetch-orders']:[],...!disposed&&scope==='public'?['subscribe']:[],...asyncOpen?['caller-resolved']:['caller-returned']];
  assert.deepEqual(trace,expected,'send reaches health notification and skips later subscriber on throw');
  assert.equal(settlement,!sends?null:listenerThrows?'rejected':'resolved','discarded reconnect promise preserves notification failure');
  assert.equal(socket.reconnecting,sends&&!listenerThrows,'notification throw skips reconnect continuation');
  assert.equal(socket.onCloseCount,sends&&!listenerThrows?101:100,'notification throw precedes close counter increment');
  if(sends)assert.equal(store.socketHealth.publicHealthy,true,'health mutation commits before notification failure');
  if(scope==='private'&&method==='onOpen'&&!asyncOpen){
   assert.equal(socket.hasOpened,!disposed,'private open continues after discarded background rejection');
   assert.equal(socket.resyncOrdersOnOpen,disposed,'private open clears resync before callback');
  }
  if(scope==='public'&&!disposed&&!market){
   assert.equal(socket.orderBookInnerState.fixture.updates.length,0,'public open resets orderbook despite background rejection');
   assert.equal(socket.orderBookInnerState.fixture.isSnapshotLoaded,false,'public open clears snapshot flag');
  }
  results.push({variant,scope,className,market,publicDataOnly,method,disposed,listenerThrows,trace,settlement,failure:observedError===null?null:{name:observedError.name,message:observedError.message,code:observedError.code},reconnecting:socket.reconnecting,onCloseCount:socket.onCloseCount,health:store.socketHealth,book:socket.orderBookInnerState.fixture,hasOpened:socket.hasOpened,resyncOrdersOnOpen:socket.resyncOrdersOnOpen});
  unsub1();unsub2();
 }
}
// Independent actual Store boundaries must forward the same original exception.
{
 const firstStore=new DefaultStore(),secondStore=new DefaultStore();
 const sentinel=Object.assign(new RangeError('nested health failure'),{code:'nested-health'});
 firstStore.subscribe(()=>{throw sentinel;});
 let firstError;try{firstStore.updateSocketHealth({publicHealthy:true,updatedAt:7});}catch(error){firstError=error;}
 assert.equal(firstError,sentinel,'source first Store retains original cause');
 secondStore.subscribe(()=>{throw firstError;});
 let secondError;try{secondStore.updateSocketHealth({publicHealthy:true,updatedAt:7});}catch(error){secondError=error;}
 assert.equal(secondError,sentinel,'source nested Store retains original cause');
 assert.equal(secondError.code,'nested-health','source nested Store preserves original metadata');
}

assert.equal(results.length,64);
process.stdout.write(JSON.stringify(results));
