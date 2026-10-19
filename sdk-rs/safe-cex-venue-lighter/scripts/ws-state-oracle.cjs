const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const clock={now:0,wall:0};
function subject(file,className,names){
 let source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter',file),'utf8');
 const mutation=process.env.LIGHTER_WS_STATE_MUTATION;
 const pair=file==='lighter.ws-private.ts'?{
  'private-ping':['STANDARD_PING_INTERVAL_MS = 10_000','STANDARD_PING_INTERVAL_MS = 10_001'],
  'toast-boundary':['now - this.privateStreamRateLimitFirstSeenAt >=','now - this.privateStreamRateLimitFirstSeenAt >'],
  'flatten-order':['Object.values(payload).flatMap','Object.values(payload).reverse().flatMap'],
  'flatten-single':["if (includeSingleObjects && entry && typeof entry === 'object')","if (false && includeSingleObjects && entry && typeof entry === 'object')"],
  'recent-clock':['Date.now() - timestampMs','virtualClock.getCurrentTimeMs() - timestampMs'],
  'reconciliation':['!this.hasReceivedAccountAllOrdersAck || this.useReadonlyFallback','!this.hasReceivedAccountAllOrdersAck && this.useReadonlyFallback'],
 }[mutation]:{'hydration-field':['ticker.mark, ticker.last, ticker.index, ticker.bid, ticker.ask','ticker.mark, ticker.last, ticker.index, ticker.bid'], 'public-cooldown':['virtualClock.getCurrentTimeMs() < this.tickerRestFallbackCooldownUntil','virtualClock.getCurrentTimeMs() <= this.tickerRestFallbackCooldownUntil']}[mutation];
 if(pair){assert.equal(source.split(pair[0]).length,2,`WS state mutation ${mutation}`);source=source.replace(...pair);}
 const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===className);assert.ok(cls,className);
 const statics=cls.members.filter(n=>ts.isPropertyDeclaration(n)&&n.modifiers?.some(m=>m.kind===ts.SyntaxKind.StaticKeyword)&&!ts.isNewExpression(n.initializer));
 const members=names.map(name=>{const member=cls.members.find(n=>n.name?.getText(tree)===name);assert.ok(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),name);return member.getText(tree);});
 const code=`class ${className} { ${statics.map(n=>n.getText(tree)).join('\n')} ${members.join('\n')} }\nexports.subject=new ${className}();`;
 const exports={};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Date:{now:()=>clock.wall},virtualClock:{getCurrentTimeMs:()=>clock.now}});return exports.subject;
}
const privateNames=['shouldEnforcePongMissReconnect','isReadonlyFallbackActive','shouldUseRestOrderReconciliation','getPingIntervalMs','getIdlePingThresholdMs','getIdleReconnectThresholdMs','getWatchdogStaleThresholdMs','isWsRateLimited','shouldForceToastPrivateStreamRateLimit','isRecentStreamEvent','flattenByMarketPayload','flattenOrdersByMarket'];
if(process.argv.includes('--close-spread')) {
 const sdk=path.resolve(__dirname,'../../../packages/safe-cex/src');
 function body(file) {
  const text=fs.readFileSync(path.join(sdk,file),'utf8');
  const tree=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
  return tree.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(tree).replace(/^export /,'')).join('\n');
 }
 const typeText=fs.readFileSync(path.join(sdk,'types.ts'),'utf8');
 const typeTree=ts.createSourceFile('types.ts',typeText,ts.ScriptTarget.Latest,true);
 const status=typeTree.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderStatus');
 assert.ok(status);
 const context=vm.createContext({structuredClone,uniqBy:require('lodash/uniqBy'),virtualClock:{getCurrentTimeMs:()=>clock.now}});
 const run=code=>vm.runInContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 run(status.getText(typeTree).replace(/^export /,'')+'\n'+body('utils/clone.ts')+'\n'+body('store/store.base.ts')+'\nglobalThis.store=new DefaultStore();');
 const store=context.store;
 const position={id:'p',symbol:'ETH',side:'long',entryPrice:10,notional:10,leverage:2,unrealizedPnl:0,contracts:1,liquidationPrice:1,marginSize:120};
 store.addOrUpdatePosition(position);
 const row=store.positions[0],revision=store.updateIds.positions;
 const socket=subject('lighter.ws-private.ts','LighterPrivateWebsocket',['handlePositionTopic']);
 socket.store=store;socket.parent={mapPosition:()=>undefined};
 socket.resolveExplicitlyClosedPositionUpdates=()=>new Set([[{id:'p'},{contracts:0,marginSize:undefined}]]).values();
 socket.handlePositionTopic([{size:'0'}]);
 assert.equal(store.positions[0],row);
 const bits=value=>{const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
 process.stdout.write(JSON.stringify({contractsBits:bits(row.contracts),marginOwn:Object.hasOwn(row,'marginSize'),marginUndefined:row.marginSize===undefined,revisionDelta:store.updateIds.positions-revision}));
 process.exit(0);
}
const privateWs=subject('lighter.ws-private.ts','LighterPrivateWebsocket',privateNames);
const publicWs=subject('lighter.ws-public.ts','LighterPublicWebsocket',['shouldEnforcePongMissReconnect','getWatchdogStaleThresholdMs','isTickerRestFallbackCoolingDown','hasHydratedTickerPrice']);
const flags=[];
for(const readonly of [false,true])for(const ack of [false,true]){Object.assign(privateWs,{useReadonlyFallback:readonly,hasReceivedAccountAllOrdersAck:ack});flags.push({readonly,ack,active:privateWs.isReadonlyFallbackActive(),reconcile:privateWs.shouldUseRestOrderReconciliation(),ping:privateWs.getPingIntervalMs(),idlePing:privateWs.getIdlePingThresholdMs(),idleReconnect:privateWs.getIdleReconnectThresholdMs(),watchdog:privateWs.getWatchdogStaleThresholdMs(),pong:privateWs.shouldEnforcePongMissReconnect()});}
const values=[NaN,Infinity,-Infinity,-0,0,-1,Number.MIN_VALUE,1,1.5,2,59999,60000,60001,Number.MAX_VALUE];
const deadlines=[];
for(const now of values)for(const until of values){clock.now=now;privateWs.wsRateLimitedUntil=until;publicWs.tickerRestFallbackCooldownUntil=until;deadlines.push({now,until,private:privateWs.isWsRateLimited(),public:publicWs.isTickerRestFallbackCoolingDown()});}
const toasts=[];
for(const now of values)for(const first of values)for(const count of values){Object.assign(privateWs,{privateStreamRateLimitErrorCount:count,privateStreamRateLimitFirstSeenAt:first});toasts.push({now,first,count,expected:privateWs.shouldForceToastPrivateStreamRateLimit(now)});}
const recent=[];
const times=[NaN,Infinity,-Infinity,-0,0,Number.MIN_VALUE,1,4999,5000,5001,10000,10001,Number.MAX_VALUE];
for(const timestamp of times)for(const wall of times)for(const virtual of [0,5000,5001,20000])for(const connected of [0,1,5000]){
 clock.now=virtual;clock.wall=wall;privateWs.lastConnectTime=connected;
 recent.push({timestamp,wall,virtual,connected,expected:privateWs.isRecentStreamEvent(timestamp)});
}
const hydration=[];
const ticker=(fields={})=>({id:'1',symbol:'BTC',mark:0,last:0,index:0,bid:0,ask:0,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0,...fields});
const hydrationInputs=[[],[ticker()]];
for(const field of ['mark','last','index','bid','ask'])for(const value of values){
 hydrationInputs.push([ticker({[field]:value})],[ticker(),ticker({[field]:value})],[ticker({[field]:value}),ticker()]);
}
for(const tickers of hydrationInputs){publicWs.parent={store:{tickers}};hydration.push({tickers,expected:publicWs.hasHydratedTickerPrice()});}
const flatten=[];
const payloads=[undefined,null,false,true,0,1,'','abc','😀',[],{},[1,null,{}],[['a'],['b']],{b:[{id:'b'}],a:[{id:'a'}],2:[{id:'2'}],1:[{id:'1'}]},JSON.parse('{"__proto__":["p"],"constructor":["c"]}')];
const entries=[null,false,0,'x',[],[null],['😀','é','\uFEFF','\u0085'],[[1]],{}, {id:'single'}, {id:1,unknown:{keep:true}}];
for(const a of entries)for(const b of entries) payloads.push({z:a,a:b},[a,b]);
const collectionIdentityInputs=require('./collection-identity.cjs').privateInputs;
assert.equal(collectionIdentityInputs.length,8,'private collection identity inventory');
payloads.unshift(...collectionIdentityInputs);
for(const payload of payloads)for(const include of [false,true])flatten.push({present:payload!==undefined,payload:payload??null,include,expected:privateWs.flattenByMarketPayload(payload,include),orders:privateWs.flattenOrdersByMarket(payload)});
const flattenBoundaries=[];
for(const input of ['\uD800','\uDC00','😀','é','\uFEFF','\u0085']){
 const payload={b:[{id:input}],a:{id:input}};
 for(const include of [false,true])flattenBoundaries.push({inputJson:JSON.stringify(payload),inputUnits:Array.from({length:input.length},(_,i)=>input.charCodeAt(i)),include,expectedJson:JSON.stringify(privateWs.flattenByMarketPayload(payload,include)),ordersJson:JSON.stringify(privateWs.flattenOrdersByMarket(payload))});
}
const nativeFlatten=[];
const bits=value=>{const bytes=new ArrayBuffer(8),view=new DataView(bytes);view.setFloat64(0,value);return view.getBigUint64(0).toString(16).padStart(16,'0');};
for(const kind of ['object','function'])for(const include of [false,true]){
 const bytes=new ArrayBuffer(8),view=new DataView(bytes);view.setBigUint64(0,0x7ff8000000000042n);
 const row={price:view.getFloat64(0),amount:Infinity,zero:-0,label:'\ud800',missing:undefined};row.cycle=row;
 const values=[,row,undefined,null];
 const payload=kind==='object'?{2:values,1:row}:Object.assign(function(){},{rows:values});
 const flattened=privateWs.flattenByMarketPayload(payload,include);
 nativeFlatten.push({kind,include,expected:{tags:flattened.map(value=>value===row?'row':value===undefined?'undefined':value===null?'null':'other'),
  dense:flattened.every((_,index)=>Object.hasOwn(flattened,index)),rowIdentity:flattened.filter(value=>value!=null).every(value=>value===row),
  nanBits:bits(row.price),zeroBits:bits(row.zero),amount:String(row.amount),units:[row.label.charCodeAt(0)],
  ownUndefined:Object.hasOwn(row,'missing')&&row.missing===undefined,cycle:row.cycle===row,
  ordersLength:privateWs.flattenOrdersByMarket(payload).length}});
}
const savedFlatten=privateWs.flattenByMarketPayload;
const dispatchPayload={},dispatchResult=[],dispatchCalls=[];
privateWs.flattenByMarketPayload=function(payload,include){dispatchCalls.push({receiver:this===privateWs,payload:payload===dispatchPayload,include});return dispatchResult;};
const flattenOrdersDispatch={same:privateWs.flattenOrdersByMarket(dispatchPayload)===dispatchResult,calls:dispatchCalls,
 stringLength:privateWs.flattenOrdersByMarket('x').length,functionLength:privateWs.flattenOrdersByMarket(function(){}).length};
privateWs.flattenByMarketPayload=savedFlatten;
if(process.env.LIGHTER_WS_CARDINALITY_MUTATION==='empty') flatten.splice(0);
if(process.env.LIGHTER_WS_CARDINALITY_MUTATION==='subset') flatten.splice(1);
process.stdout.write(JSON.stringify({flags,deadlines,toasts,recent,hydration,flatten,flattenBoundaries,nativeFlatten,flattenOrdersDispatch,publicPong:publicWs.shouldEnforcePongMissReconnect(),publicWatchdog:publicWs.getWatchdogStaleThresholdMs()},(_k,v)=>typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0))?{$number:Object.is(v,-0)?'-0':String(v)}:v));
