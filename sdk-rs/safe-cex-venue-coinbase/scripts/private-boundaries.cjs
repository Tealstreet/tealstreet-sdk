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

const witnesses=[];
for(const field of ['order_id','client_order_id']) for(const unit of [0xd800,0xdc00]) {
 const p=setup(), value=String.fromCharCode(unit), nativeField=field==='order_id'?'id':'parentId';
 const orders=Array.from({length:50},()=>raw('ordinary',{[field]:value}));
 p.send({type:'snapshot',orders});
 const key=field==='order_id'?value:'ordinary';
 assert.equal(p.socket.snapshotOrdersById.size,1,'source boundary keyed assembly size');
 assert.equal(p.socket.snapshotOrdersById.has(key),true,'source boundary Map key before encoding');
 const staged=p.socket.snapshotOrdersById.get(key)[nativeField];
 assert.equal(typeof staged,'string','source staged field type');assert.equal(staged.length,1,'source staged field length');assert.equal(staged.charCodeAt(0),unit,'source staged exact materialized unit');
 p.send({type:'snapshot',orders:[]});
 assert.equal(p.orders.length,1,'source boundary publication count');
 const published=p.orders[0][nativeField];
 assert.equal(typeof published,'string');assert.equal(published.length,1);assert.equal(published.charCodeAt(0),unit,'source published native unit before encoding');assert.equal(published,value,'source native comparison');
 witnesses.push({id:field+'-'+unit.toString(16),field,unit,mapKey:capture(key),staged:capture(staged),published:capture(published),trace:capture(p.trace)});
}
{
 const p=setup(), item=raw('ordinary');delete item.client_order_id;
 p.send({type:'update',orders:[item]});
 assert.equal(p.orders.length,1);assert.equal(Object.hasOwn(p.orders[0],'parentId'),true);assert.equal(p.orders[0].parentId,undefined,'source missing client id stays undefined');
 witnesses.push({id:'missing-client',parentOwn:true,parentUndefined:true,order:capture(p.orders[0]),trace:capture(p.trace)});
}
{
 const p=setup(), missing=raw('unused');delete missing.order_id;
 const orders=Array.from({length:50},(_,index)=>index%2===0?missing:raw('undefined'));
 p.send({type:'snapshot',orders});
 assert.equal(p.socket.snapshotOrdersById.size,2,'source undefined and literal string are separate Map keys');
 assert.equal(p.socket.snapshotOrdersById.has(undefined),true);assert.equal(p.socket.snapshotOrdersById.has('undefined'),true);
 p.send({type:'snapshot',orders:[]});
 assert.equal(p.orders.length,2);assert.equal(p.orders[0].id,undefined);assert.equal(p.orders[1].id,'undefined');
 witnesses.push({id:'missing-id-versus-literal',orders:capture(p.orders),trace:capture(p.trace)});
}
for(const [id,value] of [['product-missing',undefined],['product-null',null],['product-number',123]]) {
 const p=setup(),lookups=[];
 p.store.marketsById=new Proxy(p.store.marketsById,{get(target,key){lookups.push(String(key));return target[key];}});
 let thrown;try{p.send({type:'update',orders:[raw('ordinary',{product_id:value})]});}catch(error){thrown=error;}
 assert.ok(thrown&&thrown.name==='TypeError','source non-string product throws');
 assert.deepEqual(lookups,[String(value)],'source lookup precedes endsWith refusal');assert.equal(p.orders.length,0);
 witnesses.push({id,lookups,error:{name:thrown.name,message:thrown.message},trace:capture(p.trace)});
}
assert.equal(witnesses.length,9,'independent private boundary witness floor');
assert.deepEqual(witnesses.map(item=>item.id),['order_id-d800','order_id-dc00','client_order_id-d800','client_order_id-dc00','missing-client','missing-id-versus-literal','product-missing','product-null','product-number'],'independent private boundary witness content floor');
process.stdout.write(JSON.stringify({...captureHeader(),witnesses}));
