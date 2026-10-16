import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');const [venue,family]=process.argv.slice(2);assert.ok(['futures','pm','spot'].includes(venue));assert.ok(['start','private','tick'].includes(family));
const stem=venue==='futures'?'binance':venue==='pm'?'binancepm':'binancespot',className=venue==='futures'?'BinanceExchange':venue==='pm'?'BinancePMExchange':'BinanceSpotExchange',method=family==='private'?'_startPrivate':family;
const file=`packages/safe-cex/src/exchanges/binance/${stem}.exchange.ts`,t=ts.createSourceFile(file,fs.readFileSync(new URL('../../../'+file,import.meta.url),'utf8'),ts.ScriptTarget.Latest,true),cls=t.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===className),member=cls.members.find(n=>n.name?.getText(t)===method);assert.ok(member,method);
function decl(name){const n=t.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.ok(n,name);return n.getText(t)}
const constants=venue==='futures'&&family==='tick'?['PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS','ACCOUNT_RECONCILIATION_INTERVAL_MS','DEGRADED_ACCOUNT_RECONCILIATION_INTERVAL_MS','POSITION_RISK_RECONCILIATION_INTERVAL_MS'].map(decl).join('\n'):'';
const inputs=JSON.parse(fs.readFileSync(new URL('../tests/fixtures/lifecycle.json',import.meta.url),'utf8')).filter(f=>f.family===family);assert.equal(inputs.length,{start:22,private:18,tick:36}[family],'independent authored input floor');const results=[];
for(const fixture of inputs){
 const f=structuredClone(fixture),trace=[],effects=[],timers=[],timerCalls=[],exports={};let clocks=0,version=1,versionCalls=0,pauses=0,setterVersion=1,tickVersion=1;let disposed=!!f.disposed,publicOnly=!!f.public_only,stale=!!f.stale;
 const markets=f.name==='length-read-before-coercion'?null:[{id:'market-A'},{id:'market-B'},{id:'market-C'}],tickers=f.name==='length-read-before-coercion'?{length:{toString:0}}:f.name==='missing-length'?{}:[{id:'ticker-A',last:12.3},{id:'ticker-B',last:4.5}],orders=[{id:'order-A',amount:0.1}],risk=[{id:'risk',contracts:3.6,notional:40.2}],positions=[{id:'account',contracts:4.2,notional:50.5}],balance={total:20.5,free:16.9,used:3.6,upnl:-0.1},initialLoaded={markets:false,tickers:false,orders:false,balance:false,positions:false,nested:{keep:5}};let loaded=initialLoaded;const count=f.count;
 const context=vm.createContext({exports,Date:{now(){clocks++;trace.push(['clock']);if(f.name==='clock-fail-first'&&clocks===1||f.name==='clock-fail-risk'&&clocks===2||f.name==='clock-fail-account'&&clocks===3)throw error;return 99999+clocks;}}});
 vm.runInContext(ts.transpileModule(`${constants}\nexports.Subject=class {${member.getText(t)}};exports.failure=()=>new RangeError('lifecycle fixture');`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
 const error=exports.failure();error.code='lifecycle-code';error.response={status:503,data:{msg:'lifecycle detail'}};const subject=new exports.Subject();const body=subject[method];subject.lastPositionRiskReconciliationAt=f.risk??0;subject.lastAccountReconciliationAt=f.account??0;
 const norm=v=>v===undefined?{$undefined:true}:typeof v==='number'?(Number.isNaN(v)?'nan':v===Infinity?'infinity':v===-Infinity?'-infinity':Object.is(v,-0)?'-zero':v):Array.isArray(v)?v.map(norm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,norm(v)])):v;
 function read(name,value){trace.push(['read',name]);if(name==='loaded'&&f.name==='loaded-fail')throw error;return value}
 Object.defineProperties(subject,{needsAccountMarketOverlay:{get:()=>read('overlay',!!f.overlay)},isDisposed:{get:()=>read('disposed',disposed)},isPublicDataOnly:{get:()=>read('publicOnly',publicOnly)},ordersSnapshotStale:{get:()=>read('stale',stale)}});
 const store={get loaded(){return read('loaded',loaded)}};subject.store=store;
 function syncGetter(name){return()=>{
  trace.push(['capture',name]);if(name==='setting'&&f.name==='setter-getter-fail')throw error;
  if(name==='log'&&f.name==='log-mutates-lengths'){tickers.push({id:'ticker-C',last:6.7})}
  if(name==='update'&&f.name==='capture-version-mutation')version++;
  const receiverVersion=name==='setting'?setterVersion:1;
  return(...args)=>{trace.push(['call',name]);if(['update','log','setting','emit','publicConnect','privateConnect'].includes(name))effects.push({name,args:norm(args),receiver:receiverVersion,refs:name==='update'?{markets:args[0].markets===markets,tickers:args[0].tickers===tickers,orders:args[0].orders===orders,risk:args[0].positions===risk,positions:args[0].positions===positions,balance:args[0].balance===balance,loadedFresh:Object.hasOwn(args[0],'loaded')?args[0].loaded!==loaded:null,loadedNested:!!args[0].loaded&&args[0].loaded.nested===initialLoaded.nested}:null});
   if(name==='update'&&f.name==='update-fail'||name==='log'&&f.name==='log-fail'||name==='setting'&&f.name==='setter-call-fail'||name==='emit'&&f.name==='emit-fail'||name==='privateConnect'&&f.name==='private-connect-fail'||name==='interval'&&f.name==='interval-fail')throw error;
   if(name==='update'&&args[0].loaded)loaded=args[0].loaded;
   if(name==='publicConnect'&&f.name==='connect-disposes'||name==='privateConnect'&&f.name==='connect-disposes'||name==='log'&&f.name==='log-ready-disposes'&&String(args[0]).startsWith('Ready')||name==='log'&&f.name==='log-loaded-disposes'&&String(args[0]).startsWith('Loaded'))disposed=true;
   if(name==='paused'){pauses++;return f.name==='paused'||f.name==='paused-after-risk'&&pauses>2}
   if(name==='version'){versionCalls++;if(f.name==='version-nan')return NaN;return version}
   if(name==='interval')return 7200;
  }
 }}
 for(const [name,key]of[['update','update'],['setSetting','setting']])Object.defineProperty(store,name,{get:syncGetter(key)});
 for(const [name,key]of[['log','log'],['emitAndLogError','emit'],['isBackgroundReconciliationPaused','paused'],['getPositionStateVersion','version'],['getTickInterval','interval']])Object.defineProperty(subject,name,{get:syncGetter(key)});
 subject.publicWebsocket={};Object.defineProperty(subject.publicWebsocket,'connectAndSubscribe',{get:syncGetter('publicConnect')});subject.privateWebsocket={get isConnected(){return read('connected',f.connected??true)}};Object.defineProperty(subject.privateWebsocket,'connectAndSubscribe',{get:syncGetter('privateConnect')});
 const asyncMap=[['loadAccountMaxLeverage','overlay'],['initExternalPublicData','external'],['fetchMarkets','markets'],['fetchTickers','tickers'],['_startPrivate','startPrivate'],['_startPublicDataOnly','startPublic'],['fetchPositionMode','mode'],['tick','tick'],['fetchOrders','orders'],['retryOrdersSnapshot','retry'],['fetchPositions','positions'],['fetchPositionsAndBalance','account'],['fetchBalance','balance']];
 for(const [name,key]of asyncMap){if(name===method)continue;Object.defineProperty(subject,name,{get(){trace.push(['captureAsync',key]);return(...args)=>{trace.push(['start',key,norm(args)]);return Promise.resolve().then(()=>{trace.push(['resolve',key]);if(f.name===`${key}-fail`||f.name==='start-private-fail'&&key==='startPrivate'||f.name==='retry-fail'&&key==='retry'||f.name==='account-fail'&&['account','balance'].includes(key)||f.name==='emit-fail'&&['account','balance'].includes(key))throw error;
   if(f.name===`${key}-disposes`||f.name==='overlay-disposes'&&key==='overlay'||f.name==='tick-disposes'&&key==='tick'||f.name==='orders-disposes'&&key==='orders'||f.name==='positions-disposes'&&key==='positions'||f.name==='account-disposes'&&['account','balance'].includes(key))disposed=true;
   if(f.name==='setter-swapped'&&key==='mode')setterVersion=2;
   if(f.name==='public-route-after-await'&&key==='tickers')publicOnly=true;
   if(f.name==='version-changes'&&['positions','account'].includes(key))version++;
   return key==='external'?!!f.external:key==='markets'?markets:key==='tickers'?tickers:key==='orders'?orders:key==='positions'?risk:key==='mode'?(f.name==='mode-missing'?undefined:true):['account','balance'].includes(key)?(f.name==='malformed-account'?null:{balance,positions}):undefined;
  })}}});}
 context.loop=(fn,interval)=>{trace.push(['schedule',norm(interval)]);timers.push({fn,interval})};
 let failure;try{await body(count)}catch(e){failure=e}
 // Timer execution is explicit; the imported loop's sleep/RAF/drop policy is a boundary.
 if(timers.length){tickVersion=2;Object.defineProperty(subject,'tick',{get(){trace.push(['timerCapture',tickVersion]);if(f.name==='timer-getter-fail')throw error;if(f.name==='count-array')count.push(16);const held=tickVersion;return(...args)=>{trace.push(['timerStart',held,norm(args)]);timerCalls.push({version:held,args:norm(args)});if(f.name==='timer-call-fail')throw error;return Promise.resolve().then(()=>{trace.push(['timerResolve',held])})}}});}
 let timerFailure;for(const timer of timers){try{await timer.fn()}catch(e){timerFailure=e}}
 const fail=e=>e?{original:e===error,name:e.name,message:e.message,code:norm(e.code),response:norm(e.response)}:null;
 results.push({name:f.name,trace,effects,clocks,version,versionCalls,pauses,disposed,publicOnly,stale,loaded:norm(loaded),times:{risk:subject.lastPositionRiskReconciliationAt,account:subject.lastAccountReconciliationAt},timers:timers.map(t=>norm(t.interval)),timerCalls,failure:fail(failure),timerFailure:fail(timerFailure)});
}
process.stdout.write(JSON.stringify(results));
