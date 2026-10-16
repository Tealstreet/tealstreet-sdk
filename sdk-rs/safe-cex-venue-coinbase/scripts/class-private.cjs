const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const {captureHeader,numberBits}=require('./special-numbers.cjs');
const root=path.resolve(__dirname,'../../..');
function source(rel){const p=path.join(root,'packages',rel);return ts.createSourceFile(p,fs.readFileSync(p,'utf8'),ts.ScriptTarget.Latest,true);}
function variable(file,name){const n=file.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(n=>n.name.getText(file)===name);assert(n?.initializer,'live '+name);return `const ${name}=${n.initializer.getText(file)};`;}
const priv=source('safe-cex/src/exchanges/coinbase/coinbase.ws-private.ts'),base=source('safe-cex/src/exchanges/base.ws.ts'),util=source('safe-cex/src/exchanges/coinbase/coinbase.utils.ts'),types=source('safe-cex/src/types.ts'),math=source('safe-cex/src/utils/safe-math.ts');
function klass(file,name){const n=file.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===name);assert(n,'live '+name);return n;}
function member(file,name,key){const n=klass(file,name).members.find(n=>n.name?.getText(file)===key);assert(n,'live '+name+'.'+key);return n.getText(file);}
const text=[...types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','OrderStatus','OrderType'].includes(n.name.text)).map(n=>n.getText(types)),variable(source('math/src/isNotNumber.ts'),'isNotNumber'),math.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(math)).join('\n'),...['COINBASE_USDC_ALIAS_EXCEPTIONS','normalizeCoinbaseUsdEquivalentProductId','getCoinbaseUsdEquivalentAliasProductId','resolveCoinbaseUsdEquivalentMarket','normalizeCfmContractSize','formatOrderState','formatOrderType'].map(n=>variable(util,n)),variable(priv,'COINBASE_WS_AUTH_REQUEST_PREFIX'),variable(priv,'COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE'),`class BaseWebSocket {
 constructor(parent){this.parent=parent;} ${member(base,'BaseWebSocket','store')} ${member(base,'BaseWebSocket','emitter')}
 get isDisposed(){return this.parent.disposed;}
 trackLastReceived(){this.parent.trace.push({kind:'received'});}
 emitWsDiagnosticReceive(raw){this.parent.trace.push({kind:'diagnostic',raw});}
 emitAndLogError(message){this.parent.trace.push({kind:'error',message});}
 sendWithLog(raw){this.parent.trace.push({kind:'send',value:JSON.parse(raw)});}
 handlePongEvent(){this.parent.trace.push({kind:'pong'});}
 }`,klass(priv,'CoinbasePrivateWebsocket').getText(priv),'export {CoinbasePrivateWebsocket};'].join('\n');
let current;
const out={};vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:out,BigNumber:require('bignumber.js'),virtualClock:{getCurrentTimeMs:()=>37},console:{error:(message,value)=>current.trace.push({kind:'console',message,value})},sign:(key,secret)=>{current.trace.push({kind:'sign',key,secret,active:current.socket.snapshotOrdersById!==undefined,count:current.socket.snapshotRawOrderCount});return current.sign??Promise.resolve('jwt');}});
function capture(v){if(v===undefined)return {$undefined:true};if(typeof v==='number')return {$bits:numberBits(v)};if(typeof v==='string'&&/[\ud800-\udfff]/.test(v))return {$utf16:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(Array.isArray(v))return v.map(capture);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,capture(v)]));return v;}
function raw(id,extra={}){return {order_id:id,client_order_id:'client-'+id,product_id:'BTC-USD',cumulative_quantity:'1',leaves_quantity:'2',status:'OPEN',order_type:'Limit',order_side:'BUY',limit_price:'10',avg_price:'11',...extra};}
function setup(){
 const parent={trace:[],disposed:false,options:{key:'key',secret:'secret'},loadedRoot:{orders:false,markets:true,extra:{retained:true}},orders:[],fills:[],snapshots:[],storeHook:null,fillHook:null,parseHook:null};
 const market={symbol:'BTC-USDC',isExpiring:true,contractSize:0.5};const markets={'BTC-USDC':market};
 parent.store={marketsById:markets,get loaded(){parent.trace.push({kind:'loaded',active:parent.socket.snapshotOrdersById!==undefined,count:parent.socket.snapshotRawOrderCount});if(parent.loadedHook){const hook=parent.loadedHook;parent.loadedHook=null;hook();}return parent.loadedRoot;},update(update){parent.orders=update.orders;parent.loadedRoot=update.loaded;parent.snapshots.push(update.orders);parent.trace.push({kind:'update',ids:update.orders.map(o=>o.id),active:parent.socket.snapshotOrdersById!==undefined,count:parent.socket.snapshotRawOrderCount});if(parent.storeHook){const hook=parent.storeHook;parent.storeHook=null;hook(update.orders);}},addOrUpdateOrders(orders){parent.orders=orders;parent.trace.push({kind:'add',ids:orders.map(o=>o.id)});if(parent.storeHook){const hook=parent.storeHook;parent.storeHook=null;hook(orders);}}};
 parent.emitter={emit(name,fill){assert.equal(name,'fill');parent.fills.push(fill);parent.trace.push({kind:'fill',fill:capture(fill)});parent.fillHook?.();}};
 parent.markPrivateUserStreamMessageReceived=()=>parent.trace.push({kind:'health'});
 parent.markPrivateUserStreamAuthFailed=message=>parent.trace.push({kind:'auth',message});
 parent.socket=new out.CoinbasePrivateWebsocket(parent);current=parent;
 const parse=parent.socket.onMessage;parent.send=(event)=>{current=parent;parse({data:JSON.stringify({channel:'user',events:[event]})});};
 return parent;
}
const result={...captureHeader()};
(async()=>{
 {
 const p=setup(),socket=p.socket;const initial=Array.from({length:50},(_,i)=>raw(i===1?'b':'a'));
 p.send({type:'snapshot',orders:initial});assert.equal(socket.snapshotRawOrderCount,50);assert.equal(socket.snapshotOrdersById.size,2);assert.equal(p.snapshots.length,0);
 const before=socket.snapshotOrdersById.get('a'),beforeB=socket.snapshotOrdersById.get('b');p.fillHook=()=>{socket.snapshotOrdersById.get('a').id='renamed';};
 p.send({type:'update',orders:[raw('a',{status:'FILLED',avg_price:{native:true}})]});
 const replaced=socket.snapshotOrdersById.get('a');assert.notEqual(before,replaced);assert.equal(replaced.id,'renamed');assert.equal(socket.snapshotOrdersById.get('b'),beforeB,'b root retained across a update');assert.equal(socket.snapshotOrdersById.has('renamed'),false);
 const rawInfo=replaced.info;assert.equal(rawInfo.avg_price,p.fills[0].price,'fill retains raw avg_price object');
 p.send({type:'snapshot',orders:[raw('a',{limit_price:'23'})]});assert.equal(p.orders.length,2);assert.equal(p.orders[0].id,'a');assert.equal(p.orders[1].id,'b');assert.notEqual(p.orders[0],replaced);assert.equal(p.orders[1],beforeB,'b root retained across old-key replacement and flush');assert.equal(replaced.id,'renamed');
 result.map={orders:capture(p.orders),fills:capture(p.fills),staged:capture(replaced),trace:capture(p.trace),active:socket.snapshotOrdersById!==undefined,count:socket.snapshotRawOrderCount};
 }
 {
 const p=setup(),socket=p.socket;const nested={type:'snapshot',orders:Array.from({length:50},()=>raw('nested'))};
 p.loadedHook=()=>{assert.equal(socket.snapshotOrdersById,undefined,'reset visible before loaded getter');p.send(nested);};
 p.storeHook=()=>{assert.equal(socket.snapshotRawOrderCount,50,'loaded getter reentry survives until notify');p.send({type:'update',orders:[raw('nested',{status:'FILLED'})]});};
 p.send({type:'snapshot',orders:[raw('old')]});assert.equal(socket.snapshotRawOrderCount,50,'older flush must retain newer assembly');assert.equal(socket.snapshotOrdersById.get('nested').info.status,'FILLED');assert.equal(p.snapshots.length,1);assert.equal(p.fills.length,1);
 const staged=socket.snapshotOrdersById.get('nested');p.send({type:'snapshot',orders:[]});assert.equal(p.snapshots.length,2);assert.equal(p.orders[0],staged);assert.equal(socket.snapshotOrdersById,undefined);assert.equal(socket.snapshotRawOrderCount,0);
 result.reentry={orders:capture(p.orders),snapshots:capture(p.snapshots),fills:capture(p.fills),trace:capture(p.trace)};
 }
 {
 const p=setup(),socket=p.socket;p.send({type:'snapshot',orders:[raw('old')]});
 let touched=false;const book=p.store.marketsById;Object.defineProperty(book,'BTC-USD',{get(){if(!touched){touched=true;event.orders.push(raw('appended',{cumulative_quantity:'1\ud800',limit_price:'4\ud800'}));event.type='snapshot';event.orders=[];}return undefined;}});
 const event={type:'update',orders:[raw('live')]};current=p;socket.handleUserEventOrders(event);
 assert.equal(p.orders.length,2);assert.equal(p.orders[0].id,'live');assert.equal(p.orders[1].id,'appended');assert.equal(p.orders[1].filled,0.5);assert.equal(p.orders[1].price,4);assert.equal(socket.snapshotOrdersById,undefined);
 result.live={orders:capture(p.orders),trace:capture(p.trace)};
 }
 {
 const p=setup(),socket=p.socket;p.send({type:'snapshot',orders:Array.from({length:50},()=>raw('pending'))});
 let resolve;p.sign=new Promise(done=>resolve=done);const pending=socket.onOpen();assert.equal(socket.subscriptionAttempt,1);assert.equal(socket.snapshotOrdersById,undefined,'reset occurs before suspended signer');assert.equal(socket.snapshotRawOrderCount,0);assert.equal(p.trace.at(-1).kind,'sign');resolve('jwt');await pending;
 assert.equal(p.trace.filter(v=>v.kind==='send').length,2);p.disposed=true;const reads=p.trace.length;await socket.onOpen();assert.equal(p.trace.length,reads);assert.equal(socket.subscriptionAttempt,1);
 result.open={trace:capture(p.trace),attempt:socket.subscriptionAttempt,active:socket.snapshotOrdersById!==undefined,count:socket.snapshotRawOrderCount};
 }
 {
 const p=setup();p.storeHook=orders=>{const order=orders[0];order.id='store-id';order.symbol='store-symbol';order.filled=9;order.info.avg_price=[];orders.push(order);};
 p.fillHook=()=>{if(p.fills.length===1){p.orders.push(p.orders[0]);p.orders[0].id='fill-id';}};
 p.send({type:'update',orders:[raw('updating',{status:'FILLED'})]});
 assert.equal(p.orders.length,3);assert.equal(p.orders[0],p.orders[1]);assert.equal(p.orders[1],p.orders[2]);assert.equal(p.fills.length,2,'fill forEach captures post-Store length before callback append');assert.equal(p.fills[0].id,'store-id');assert.equal(p.fills[1].id,'fill-id');assert.equal(p.fills[0].price,p.orders[0].info.avg_price);assert.equal(p.fills[1].price,p.fills[0].price);
 result.update={orders:capture(p.orders),fills:capture(p.fills),trace:capture(p.trace)};
 }
 {
 const p=setup(),socket=p.socket;
 p.send({type:'snapshot',orders:Array.from({length:50},()=>raw('pending'))});
 const cause=new Error('snapshot listener failed');cause.response={status:409,data:{message:'native listener metadata'}};
 let subscriberCalls=0;
 p.storeHook=()=>{subscriberCalls++;assert.equal(socket.snapshotOrdersById,undefined,'source snapshot reset before subscriber');assert.equal(socket.snapshotRawOrderCount,0,'source raw count reset before subscriber');throw cause;};
 let thrown;
 try{socket.onMessage({data:JSON.stringify({channel:'user',events:[{type:'snapshot',orders:[raw('one')]},{type:'update',orders:[raw('must-not-run')]}]})});}catch(error){thrown=error;}
 assert.equal(thrown,cause,'source snapshot retains original thrown subscriber object');
 assert.deepEqual(Array.from(p.orders,order=>order.id),['pending','one']);
 assert.equal(p.trace.filter(item=>item.kind==='add').length,0,'source snapshot stops following user event');
 const observed={sameCause:thrown===cause,ids:p.orders.map(order=>order.id),loaded:p.loadedRoot.orders,active:socket.snapshotOrdersById!==undefined,count:socket.snapshotRawOrderCount,publications:p.snapshots.length,fills:p.fills.length,followingEvents:p.trace.filter(item=>item.kind==='add').length,subscriberCalls};
 p.send({type:'snapshot',orders:[]});observed.recovered=p.orders.length===0&&p.loadedRoot.orders;
 result.notification=observed;
 }
 process.stdout.write(JSON.stringify(result));
})().catch(error=>{console.error(error);process.exit(1);});
