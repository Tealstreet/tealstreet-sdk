// Execute maintained exchange/private classes with inherited runtime/HTTP capabilities and a public-listener spy.
// Public stream bodies have their separate oracle; native exchange construction owns the actual public facade.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler_required');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const venue=path.join(root,'packages/safe-cex/src/exchanges/blofin');
function source(file,name){const text=fs.readFileSync(path.join(venue,file),'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text===name);assert(klass,name);return {ast,klass};}
const exchange=source('blofin.exchange.ts','BlofinExchange'),priv=source('blofin.ws-private.ts','BlofinPrivateWebsocket');
const sourceMembers=exchange.klass.members.map(m=>ts.isConstructorDeclaration(m)?'constructor':m.name?.text).filter(Boolean);
assert.equal(sourceMembers.length,71,'independent full exchange member floor');
const sourcePrivate=priv.klass.members.map(m=>m.name?.text).filter(Boolean);
assert.equal(sourcePrivate.length,16,'independent full private member floor');
const shared={...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(venue,'blofin.types.ts')),...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/utils/incrementalFills.ts')),...load(path.join(root,'packages/safe-cex/src/utils/roundUSD.ts')),...load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts')),...load(path.join(root,'packages/safe-cex/src/utils/inverseObj.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/clientOrderId.ts')),...load(path.join(root,'packages/json-parse/src/jsonParse.ts'))};
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
for(const name of ['sumBy','flatten','partition','times','uniqBy','chunk'])shared[name]=dependencies('lodash/'+name);
function constructClass(unit,name,context){const vars=unit.ast.statements.filter(ts.isVariableStatement).map(s=>s.getText(unit.ast)).join('\n');const code=vars+'\n'+unit.klass.getText(unit.ast).replace(/^export /,'')+'\nreturn '+name+';';return new Function(...Object.keys(context),ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(context));}
function observe(v){if(typeof v==='number'){if(Number.isNaN(v))return{number:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:b.toString('hex')};}if(Array.isArray(v))return Array.from(v,observe);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,observe(v)]));return v;}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,contractSize:0.125,info:{contractValue:'0.375'},precision:{amount:0.01,price:0.25},limits:{amount:{min:1,max:999},leverage:{min:2,max:17}}};
const order={instId:market.id,instType:'SWAP',orderId:'o1',ordId:'ord-alt',orderType:'limit',orderCategory:'normal',side:'buy',state:'partially_filled',size:'8',filledSize:'2',price:'100',averagePrice:'101.25',createTime:'1710000000000',uTime:'1710000001123',reduceOnly:'false',positionSide:'net'};
const position={instId:market.id,instType:'SWAP',positionId:'p1',positions:'8',positionSide:'net',averagePrice:'100',markPrice:'102',liquidationPrice:'80',leverage:'4',marginMode:'cross',unrealizedPnl:'2.75',maintenanceMargin:'3.25'};
const balance={free:10,total:20,used:3.25,upnl:2.75};
const fixtures=[],add=(group,actions,initial={})=>fixtures.push({group,actions,initial});
add('orders',[['orders',[order]],['orders',[order]],['orders',[{...order,filledSize:'2.2',averagePrice:'101.3'}]],['orders',[{...order,filledSize:'1'}]]]);
for(const state of ['live','filled','canceled','effective','order_failed','unknown'])add('orders',[['orders',[{...order,state}]]]);
for(const filledSize of ['0','-1','bad','Infinity','2tail'])add('orders',[['orders',[{...order,filledSize}]]]);
for(const averagePrice of ['bad','0','-5','Infinity','102tail'])add('orders',[['orders',[{...order,averagePrice}]]]);
for(const fillTime of ['1000000000000','1000000000001','1710000000123.9','-1.2','bad',[],null])add('orders',[['orders',[{...order,fillTime,uTime:'',cTime:''}]]]);
add('orders',[['orders',[{...order,state:'filled'}]],['orders',[{...order,state:'filled'}]]]);
add('orders',[['orders',[order,{...order,orderId:'o2',filledSize:'4'}]]],{fail:'fill'});
add('orders',[['orders',[order]]],{contractSize:0,contractValue:'bad'});
add('orders',[['orders',[order]]],{missingMarket:true});
add('orders',[['orders',[]]]);add('orders',[['orders','🚀a']]);
add('positions',[['positions',[position]]]);
add('positions',[['positions',[position,{...position,instId:'missing',maintenanceMargin:'4.5',unrealizedPnl:'-1.25'}]]]);
add('positions',[['positions',[{...position,maintenanceMargin:'bad',unrealizedPnl:'bad'}]]]);
add('positions',[['positions',[]]]);add('positions',[['positions',[{...position,positions:'0'}]]]);
add('positions',[['positions',[{...position,maintenanceMargin:'-0',unrealizedPnl:'-0'}]]]);
add('positions',[['positions',[position]]],{mappingError:true});
for(const totalEquity of ['102.499','bad','Infinity','-0',null,['40.125'],'4tail'])add('account',[['account',{totalEquity}]]);
add('account-alias',[['account',{totalEquity:'100'}],['mutate-account']]);
add('orders-mapper-alias',[['orders',[order]]],{mutateMapper:true});
add('private',[['message','{"arg":{"channel":"account"},"data":{"totalEquity":"100"}}'],['message','{"arg":{"channel":"positions"},"data":'+JSON.stringify([position])+'}'],['message','{"arg":{"channel":"orders"},"data":'+JSON.stringify([order])+'}'],['dispose']]);
add('private-replace',[['replace-account'],['message','{"arg":{"channel":"account"},"data":{"totalEquity":"100"}}']]);
for(const data of [null,{},1])add('malformed',[['orders',data]]);
for(const data of [null,{}])add('malformed',[['positions',data]]);
add('malformed',[['account',null]]);
for(const channel of ['orders','positions','account'])add('malformed',[['message',JSON.stringify({arg:{channel}})]]);
for(const topic of ['orders','positions','account'])add('malformed',[['root-'+topic,null]]);
add('positions',[['positions',[{...position,maintenanceMargin:{toString:0}}]]]);
for(const state of ['filled','canceled','effective','order_failed'])add('terminal',[['orders',[order]],['orders',[{...order,state,filledSize:'3'}]]]);
add('private-dispose',[['dispose'],['message','{"arg":{"channel":"account"},"data":{"totalEquity":"100"}}']]);
const floors={orders:30,terminal:4,positions:8,account:7,'account-alias':1,'orders-mapper-alias':1,private:1,'private-replace':1,malformed:12,'private-dispose':1};
assert.equal(fixtures.length,66,'independent66floor');for(const [g,n]of Object.entries(floors))assert.equal(fixtures.filter(f=>f.group===g).length,n,g+'floor');
const apiSource=ts.createSourceFile('blofin.api.ts',fs.readFileSync(path.join(venue,'blofin.api.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const apiInitializer=apiSource.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text==='createAPI').initializer;
function sourceApi(safeCexAxiosCreate){const context={AxiosHeaders:dependencies('axios').AxiosHeaders,retry:()=>{},createHmac:dependencies('create-hmac'),qs:dependencies('qs'),uuid:()=> '0123456789abcdef0123456789abcdef',virtualClock:{getCurrentTime:()=>1710000000000.125},networkErrorOnRead:()=>()=>false,BASE_URL:shared.BASE_URL,BLOFIN_AUTH_ERROR_CODES:new Set(['152401','152406']),safeCexAxiosCreate};return new Function(...Object.keys(context),ts.transpileModule('return '+apiInitializer.getText(apiSource),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(context));}
function runtime(initial={}){
 const store=new DefaultStore();const m=structuredClone(market);if(Object.hasOwn(initial,'contractSize'))m.contractSize=initial.contractSize;if(initial.contractValue)m.info.contractValue=initial.contractValue;
 store.update({markets:initial.missingMarket?[]:[m],balance:{...balance},loaded:{markets:true,tickers:false,positions:false,orders:false,balance:true}});
 const trace=[],fills=[],fillStatuses=[],timers=new Map();let next=1,rawAccount;
 const fail=label=>{if(initial.fail===label)throw Error(label+' failed')};
 class BaseWebSocket{constructor(parent){this.parent=parent;this.store=store;this.isDisposed=false;trace.push(['socket-construct']);this.emitter={emit:(kind,fill)=>{fail(kind);fills.push({...fill});fillStatuses.push(store.orders.find(o=>o.id===fill.orderId)?.status??null);trace.push(['fill'])}};}
 dispose(){trace.push(['socket-dispose']);this.isDisposed=true;}trackLastReceived(){trace.push(['track'])}emitWsDiagnosticReceive(){trace.push(['receive'])}logLastLog(){trace.push(['log'])}handlePongEvent(){trace.push(['pong'])}sendWithLog(frame){trace.push(['send',frame])}emitAndLogError(msg){trace.push(['mapping-error',msg])}}
 class BaseExchange{constructor(name,options,s){this.name=name;this.options=options;this.store=s;this.isDisposed=false;trace.push(['base',name]);}createXhrWithSecondaryKey(opts,factory){assert.equal(factory,api);trace.push(['xhr']);return factory(opts,this);}recordAuthFailure(){this._authFailureStreak=(this._authFailureStreak??0)+1;}recordAuthSuccess(){this._authFailureStreak=0;}dispose(){trace.push(['base-dispose']);this.isDisposed=true;}emitAndLogError(message){trace.push(['mapping-error',message])}assertSingleNativeClientOrderId(){ }resolveCallerClientOrderId(){return undefined;}assertCallerClientOrderIdsForBatch(){}}
 const requestInterceptors=[],responseInterceptors=[],signedRequests=[];
 const xhr={interceptors:{request:{use:fn=>requestInterceptors.push(fn)},response:{use:fn=>responseInterceptors.push(fn)}},get:async(url,config={})=>{trace.push(['get',url]);let request={...config,url,method:'get',headers:{}};for(const fn of requestInterceptors)request=fn(request);signedRequests.push(request);let response={config:request,data:{code:'0',data:[]}};for(const fn of responseInterceptors)response=fn(response);return response;},post:async()=>({data:{code:'0',data:[]}})};
 const api=sourceApi(config=>{assert.equal(config.baseURL,'https://openapi.blofin.com');assert.equal(config.timeout,20000);return xhr;});

 class Public{constructor(parent){this.parent=parent;this.treatAnyMessageAsPong=true;trace.push(['public']);}dispose(){trace.push(['public-dispose'])}listenOHLCV(opts,callback){this.lastOpts=opts;this.lastCallback=callback;return this.cleanup;}listenOrderBook(symbol,callback){this.lastSymbol=symbol;this.lastCallback=callback;return this.cleanup;}}
 const context={...shared,BaseWebSocket,BaseExchange,createAPI:api,BlofinPublicWebsocket:Public,uuid:()=> '0123456789abcdef0123456789abcdef',nanoid:()=> 'nano',virtualClock:{getCurrentTimeMs:()=>1710000000000.125},Date:{now:()=>1710000000555},performance:{now:()=>123.5},setTimeout:(cb,delay)=>{const id=next++;timers.set(id,cb);trace.push(['timer',id,delay]);return id;},clearTimeout:id=>{timers.delete(id);trace.push(['clear',id])},ENABLE_RECONNECTING_WEBSOCKET:true,resolveTransportRouting:()=>({wsPrivate:{tunnelled:false}}),createHmac:dependencies('create-hmac')};
 const Private=constructClass(priv,'BlofinPrivateWebsocket',context);context.BlofinPrivateWebsocket=Private;
 const Exchange=constructClass(exchange,'BlofinExchange',context);
 const parent=new Exchange('blofin',{key:'k',secret:'s',passphrase:'p'},store),stream=parent.privateWebsocket;
 if(initial.mutateMapper){const real=parent.mapOrders;parent.mapOrders=rows=>{const result=real(rows);rows[0].filledSize='4';return result;};}
 if(initial.mappingError)parent.mapPositions=()=>{throw Error('mapping failed')};
 const constructorTrace=structuredClone(trace);trace.length=0;
 return{constructorTrace,signedRequests,store,trace,fills,fillStatuses,timers,parent,stream,setAccount:v=>{rawAccount=v},mutateAccount:()=>{rawAccount.totalEquity='999'}};
}
function run(f){const input=structuredClone(f),r=runtime(f.initial),errors=[];for(const [action,arg]of f.actions){try{if(action==='root-orders')r.stream.handleOrderTopic(arg);else if(action==='root-positions')r.stream.handlePositionTopic(arg);else if(action==='root-account')r.stream.handleAccountTopic(arg);else if(action==='orders')r.stream.handleOrderTopic({data:structuredClone(arg)});else if(action==='positions')r.stream.handlePositionTopic({data:structuredClone(arg)});else if(action==='account'){const d=structuredClone(arg);r.setAccount(d);r.stream.handleAccountTopic({data:d});}else if(action==='mutate-account')r.mutateAccount();else if(action==='replace-account')r.stream.handleAccountTopic=()=>r.trace.push(['replacement']);else if(action==='message')r.stream.onMessage({data:arg});else if(action==='dispose')r.stream.dispose();}catch(e){errors.push(e.message);}}
 return{...input,orders:observe(r.store.orders),positions:observe(r.store.positions),balance:observe(r.store.balance),fills:observe(r.fills),fillStatuses:r.fillStatuses,parentDisposed:r.parent.isDisposed,privateDisposed:r.stream.isDisposed,trace:r.trace,errors};}
const cases=fixtures.map(run);
const decoded=o=>Buffer.from(o.number,'hex').readDoubleBE();assert.deepEqual(cases[0].fills.map(f=>[decoded(f.amount),decoded(f.price)]),[[0.25,101.25],[0.025,101.8]],'each partial at its own price without float noise');
assert.equal(cases[0].fills.length,2,'dedupe suppresses repeat and regression');assert.deepEqual(cases[24].actions.map(([,rows])=>rows[0].state),['filled','filled'],'replay fixture');assert.equal(cases[24].fills.length,1,'replayed filled frame emits once');assert.equal(cases.find(c=>c.group==='orders-mapper-alias').fills[0].amount.number,'3fe0000000000000','source mapper alias changes fill to0.5');assert.equal(cases.find(c=>c.group==='account-alias').balance.info.totalEquity,'999','source retains account info alias');
const composition=runtime();const p=composition.parent,cleanup=()=>{},callback=()=>{},opts={symbol:'BTCUSDT',interval:'1m'};p.publicWebsocket.cleanup=cleanup;assert.equal(p.listenOHLCV(opts,callback),cleanup);assert.equal(p.publicWebsocket.lastOpts,opts);assert.equal(p.publicWebsocket.lastCallback,callback);assert.equal(p.listenOrderBook('BTCUSDT',callback),cleanup);assert.equal(p.publicWebsocket.lastCallback,callback);p.options.testnet=true;const liveEndpoint=p.privateWebsocket.endpoint;const ctorTrace=runtime().parent;assert.equal(ctorTrace._accountMarginType,null);assert.equal(ctorTrace._accountPositionMode,null);p.dispose();assert.deepEqual(composition.trace.slice(-3),[['base-dispose'],['public-dispose'],['socket-dispose']]);
const baseFile=path.join(root,'packages/safe-cex/src/exchanges/base.ts');
const baseAst=ts.createSourceFile(baseFile,fs.readFileSync(baseFile,'utf8'),ts.ScriptTarget.Latest,true);
const baseClass=baseAst.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BaseExchange');
const tickMember=baseClass.members.find(m=>m.name?.text==='getTickInterval');assert(tickMember,'inherited tick policy');
const TickBase=new Function('getExchangeMetadata','getTickIntervalMultiplier','AUTH_FAILURE_MAX_TICK_INTERVAL',ts.transpileModule('return class {'+tickMember.getText(baseAst)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(()=>({tickInterval:2000}),()=>1,60000);
const tickBase=new TickBase();tickBase.options={};tickBase._exchangeName='blofin';const backoff=[];for(const streak of [0,2,7]){tickBase._authFailureStreak=streak;backoff.push(tickBase.getTickInterval());}assert.deepEqual(backoff,[2000,8000,60000],'actual inherited backoff policy');
const privateDisposal=runtime();privateDisposal.stream.dispose();privateDisposal.stream.onMessage({data:'{"arg":{"channel":"account"},"data":{"totalEquity":"100"}}'});assert.equal(privateDisposal.store.balance.total,20);assert.equal(privateDisposal.parent.isDisposed,false);assert.equal(privateDisposal.parent.mapOrders([order]).length,1,'parent survives private disposal');const publicCleanup=()=>{};privateDisposal.parent.publicWebsocket.cleanup=publicCleanup;assert.equal(privateDisposal.parent.listenOrderBook('BTCUSDT',()=>{}),publicCleanup);
const replacement=runtime();replacement.parent.mapOrders=()=>[{id:'runtime-replacement'}];
let releasedParent=runtime().parent;const retainedPrivate=releasedParent.privateWebsocket;releasedParent=null;
assert.equal(retainedPrivate.parent.mapOrders([order]).length,1,'source private retains parent');
replacement.parent.fetchNormalOrders().then(staticMemberResult=>console.log(JSON.stringify({market,order,position,balance,sourceMembers,sourcePrivate,cases,stoppedAfter:66,forwarding:{cleanupIdentity:true,optionsIdentity:true,callbackIdentity:true},liveEndpoint,constructorTrace:composition.constructorTrace,sourceMint:ctorTrace.clientOrderIdRule.mint(),staticMemberResult,retainedParentOrders:retainedPrivate.parent.mapOrders([order]).length,backoff,signedRequest:replacement.signedRequests[0]}))).catch(e=>{console.error(e);process.exitCode=1;});
