'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),hashes={};
function source(file){const s=fs.readFileSync(path.join(root,'packages/safe-cex/src',file),'utf8');hashes[file]=crypto.createHash('sha256').update(s).digest('hex');return ts.createSourceFile(file,s,ts.ScriptTarget.Latest,true);}
function evaluate(s,b={}){const r=ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal((r.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);return vm.runInNewContext(r.outputText,{exports:{},...b});}
function constant(a,n,b={}){const d=a.statements.filter(ts.isVariableStatement).flatMap(x=>[...x.declarationList.declarations]).find(x=>x.name.getText(a)===n);assert.ok(d,n);return evaluate('('+d.initializer.getText(a)+')',b);}
const types=source('exchanges/orderly/orderly.types.ts'),constants={};for(const n of ['BASE_URL','BROKER_ID','TESTNET_BROKER_ID','ENDPOINTS','RECV_WINDOW'])constants[n]=constant(types,n);
const api=source('exchanges/orderly/orderly.api.ts'),pub=source('exchanges/orderly/orderly.ws-public.ts'),priv=source('exchanges/orderly/orderly.ws-private.ts');
const classes=[pub,priv].map((a,i)=>a.statements.find(x=>ts.isClassDeclaration(x)&&x.name.text===(i?'OrderlyPrivateWebSocket':'OrderlyPublicWebsocket')));
function member(n,b,r){const d=classes[0].members.find(x=>x.name?.getText(pub)===n);assert.ok(d);return evaluate('function make(){return ('+d.initializer.getText(pub)+')} ;make',b).call(r);}
const lodash=n=>require(path.join(root,'node_modules/lodash',n)),reverseSymbol=constant(source('exchanges/orderly/orderly.utils.ts'),'reverseSymbol',{memoize:lodash('memoize')});
const utility=source('utils/updateOrderBook.ts'),binarySearchPrice=constant(utility,'binarySearchPrice'),updateOrderBook=constant(utility,'updateOrderBook',{binarySearchPrice}),calcOrderBookTotal=constant(source('utils/orderbook.ts'),'calcOrderBookTotal');
const qs=require(path.join(root,'node_modules/qs'));
function capture(v){if(v===undefined)return {undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(typeof v==='function')return {function:v.name};if(Array.isArray(v))return v.map(capture);if(v&&typeof v==='object')return {entries:Object.entries(v).map(([k,x])=>[k,capture(x)])};return v;}
const snapshot=v=>structuredClone(capture(v));
const defaults={options:{secret:'seed',key:'public-key',applicationId:'account',extra:{}},config:{method:'post',url:'/v1/order',params:{z:'last',a:'with space'},data:{z:2,a:1},headers:{Accept:'old',custom:'x'}},connected:true,symbol:'BTC/USDT:USDT',interval:'1m',snapshot:{timestamp:100,bids:[{price:10,quantity:2},{price:9,quantity:3}],asks:[{price:11,quantity:4}]},updates:[{ts:99,data:{bids:[[10,99]],asks:[]}},{ts:100,data:{bids:[[9,88]],asks:[]}},{ts:101,data:{bids:[[10,5],[8,2]],asks:[[11,0],[12,3]]}},{ts:102,data:{bids:[[10,0],[9,6]],asks:[[12,7]]}}]};
const cases={
 sign:[{id:'post-query-order'},{id:'default-get',config:{url:'/v1/account',headers:{}}},{id:'delete-empty',config:{method:'delete',url:'/v1/order',data:{},params:{}}},{id:'optional-nonmethod',config:{method:5,url:null,data:false}},{id:'nullish-join',config:{}},{id:'raw-array',config:{method:'put',url:'/x',data:[1,'x'],headers:{}}},{id:'sign-error',fail:'sign'},{id:'headers-error',fail:'headers'},{id:'url-coercion-before-query',config:{url:{throwString:true},params:{a:1}}}],
 api:[{id:'default',config:{url:'/v1/public/info'}},{id:'zero-timeout',options:{secret:'s',key:'k',applicationId:42,extra:{recvWindow:0}}},{id:'testnet-tunnel',options:{secret:'s',key:'k',testnet:true,tealstreetTunnel:true,extra:{recvWindow:5}}},{id:'public-identity',config:{url:'/v1/public/info'}},{id:'algo-mutable-options',mutate:true,config:{method:'PoSt',url:'/v1/algo/order',data:{x:1}}},{id:'retry-error',fail:'retry'},{id:'create-error',fail:'create'},{id:'transport-error',fail:'transport',options:{secret:'s',key:'k',tealstreetTunnel:true}}],
 candles:[{id:'connected'},{id:'second-subscriber',second:true},{id:'retry-after-cleanup',connected:false},{id:'disposed-retry',connected:false,disposed:true},{id:'raw-candle',candle:{startTime:'2500',open:'1',high:null,low:false,close:0,volume:'2'}},{id:'callback-error',fail:'callback'}],
 book:[{id:'buffer-filter-live'},{id:'cleanup-pending',cleanupPending:true},{id:'disposed-pending',disposePending:true},{id:'request-retry',reject:true},{id:'sync-request-error',fail:'request'},{id:'callback-error',fail:'callback'},{id:'logger-error',reject:true,fail:'error'},{id:'disconnected-cleanup',connected:false},{id:'second-subscriber',second:true}],
 process:[{id:'both-sides'},{id:'empty',updates:[{data:{bids:[],asks:[]}}]},{id:'raw-string',updates:[{data:{bids:[['10','5'],['10',0]],asks:[[12,-2]]}}]}],
 public:[{id:'shape-pair',pair:true},{id:'no-args',noArgs:true},{id:'super-error',fail:'super'}],
 private:[{id:'shape-pair',pair:true},{id:'no-args',noArgs:true},{id:'super-error',fail:'super'}]
};
async function run(group,raw){const c=structuredClone({...defaults,...raw}),input=capture(c),events=[],original={message:'original failure'},timers=[],pending=[];let interceptor,client,apiConfig,lastBook,detached=[];
 const effect=(kind,fields={})=>{events.push({kind,...fields});if(c.fail===kind)throw original;};
 if(c.config.url?.throwString)Object.defineProperty(c.config.url,'toString',{value:function(){effect('url-string');throw original;}});
 const bindings={...constants,qs:{stringify(params,opts){effect('query',{params,sorted:!!opts});return qs.stringify(params,opts);}},virtualClock:{getCurrentTimeMs(){effect('clock');return 1234;}},orderlySign(message,secret){effect('sign',{message,secret,secretIdentity:secret===c.options.secret});return 'signature';},AxiosHeaders:class{constructor(fields){effect('headers',{fields});Object.assign(this,fields);}},safeCexAxiosCreate(config,options,exchange){effect('create',{config,optionsIdentity:options===c.options,name:exchange.name});apiConfig=config;client={interceptors:{request:{use:function use(f){effect('use',{receiverIdentity:this===client.interceptors.request});interceptor=f;}}}};return client;},networkErrorOnRead(){effect('condition');return 'condition';},retry(x,o){effect('retry',{clientIdentity:x===client,options:o});},installTransportAdapter(x,o,n){effect('transport',{clientIdentity:x===client,optionsIdentity:o===c.options,name:n});}};
 const signRequest=constant(api,'signRequest',bindings);bindings.signRequest=signRequest;const createAPI=constant(api,'createAPI',bindings);
 const timer=(f,d)=>{const id=timers.length+1;timers.push({f,cleared:false});effect('timer',{id,delay:d});return id;};
 const feedBindings={...constants,reverseSymbol,updateOrderBook,calcOrderBookTotal,setTimeout:timer,clearTimeout(id){timers[id-1].cleared=true;effect('clear',{id});}};
 const r={isConnected:c.connected,isDisposed:!!c.disposed,messageHandlers:{},topics:{},send:function send(frame){effect('send',{frame});},cleanupMessageHandlerStats:function cleanup(topic){effect('cleanup',{topic});},emitAndLogError:function error(message){effect('error',{message});},parent:{xhr:{get:function request(url){effect('request',{url});return new Promise((resolve,reject)=>pending.push({resolve,reject}));}}}};
 r.processOrderBookUpdate=member('processOrderBookUpdate',feedBindings,r);
 const callback=function callback(value){effect('callback',{value:snapshotValue(value),sameBook:group==='book'?(!lastBook||lastBook===value):false});lastBook=value;};
 function snapshotValue(v){return v===undefined?undefined:structuredClone(v);}
 let result,error,identity;
 const drain=async()=>{for(let n=0;n<4;n++)await Promise.resolve();};
 const rejected=e=>{detached.push(e);};process.on('unhandledRejection',rejected);
 try{
 if(group==='sign'){result=signRequest(c.config,c.options);identity={fresh:result!==c.config,nestedData:result.data===c.config.data,paramsOriginal:Object.hasOwn(c.config,'params')};}
 if(group==='api'){result=createAPI(c.options,{name:'Orderly'});identity={client:result===client};effect('serialize',{value:apiConfig.paramsSerializer.serialize({z:'last',a:'with space'})});if(c.mutate){c.options.testnet=true;c.options.secret='changed';}const v=interceptor(c.config);identity.publicOriginal=v===c.config;result=v;}
 if(group==='candles'||group==='book'){
  const topic=reverseSymbol(c.symbol)+(group==='book'?'@orderbookupdate':'@kline_'+c.interval),listen=member(group==='book'?'listenOrderBook':'listenOHLCV',feedBindings,r),cleanup=listen(group==='book'?c.symbol:{symbol:c.symbol,interval:c.interval},callback);let second;
  if(c.second)second=listen(group==='book'?c.symbol:{symbol:c.symbol,interval:c.interval},callback);
  if(!c.connected){cleanup();r.isConnected=true;for(const t of timers.slice())if(!t.cleared)t.f();}
  else if(group==='candles'){for(const parser of (r.messageHandlers[topic]||[]).slice())parser({data:c.candle||{startTime:2500,open:1,high:3,low:0,close:2,volume:4}});cleanup();if(second)second();cleanup();}
  else{
   const parser=r.messageHandlers[topic][0];parser(c.updates[0]);parser(c.updates[1]);parser(c.updates[2]);
   if(c.cleanupPending)cleanup();if(c.disposePending)r.isDisposed=true;
   if(pending.length){if(c.reject){pending.shift().reject(original);await drain();parser(c.updates[2]);if(pending.length)pending.shift().resolve({data:{data:c.snapshot}});}else pending.shift().resolve({data:{data:c.snapshot}});await drain();}
   if(!c.cleanupPending&&!c.disposePending){parser(c.updates[3]);cleanup();if(second)second();}
  }
  await drain();result={handlers:r.messageHandlers,topics:r.topics,book:lastBook};
 }
 if(group==='process'){const book={bids:[],asks:[]};for(const update of c.updates)r.processOrderBookUpdate(book,update);result=book;}
 if(group==='public'||group==='private'){
  const privateSocket=group==='private',ast=privateSocket?priv:pub,cls=classes[privateSocket?1:0];
  class BaseWebSocket{constructor(...args){effect('super',{private:privateSocket,argCount:args.length,parentIdentity:args[0]===c.options});this.parent=args[0];}}
  const Class=evaluate(cls.getText(ast)+';exports.'+cls.name.text,{...feedBindings,BaseWebSocket});
  const args=c.noArgs?[]:[c.options,{extra:1}],first=new Class(...args);result=shape(first);identity={};
  if(c.pair){const second=new Class(...args);identity.fresh=Object.keys(first).filter(k=>typeof first[k]==='function'||k==='topics'||k==='messageHandlers').every(k=>first[k]!==second[k]);
   if(!privateSocket){for(const [topic,method,data]of [['ping','handlePingEvent',false],['pong','handlePongEvent',false],['tickers','handleTickersStreamEvents',true],['bbos','handleBBOStreamEvents',true],['markprices','handleMarkPricesStreamEvents',true],['estfundingrate','handleFundingRateStreamEvents',true]]){first[method]=function replaced(...args){effect('dispatch',{method,receiverIdentity:this===first,argCount:args.length,dataIdentity:!data||args[0]===c.options});};Reflect.apply(first.messageHandlers[topic][0],second,[{data:c.options}]);}}
  }
 }
 }catch(e){error={identity:e===original,message:e.message};}
 await new Promise(resolve=>setImmediate(resolve));process.removeListener('unhandledRejection',rejected);for(const e of detached)effect('detached',{identity:e===original,message:e.message});
 return {cfg:input,result:capture(result),error:capture(error),identity:capture(identity),events:capture(events),configAfter:capture(c.config),optionsAfter:capture(c.options)};
}
function shape(r){return Object.getOwnPropertyNames(r).map(key=>{const d=Object.getOwnPropertyDescriptor(r,key);return {key,writable:d.writable,enumerable:d.enumerable,configurable:d.configurable,value:d.value};});}
(async()=>{const rows={};for(const[g,cs]of Object.entries(cases)){rows[g]=[];for(const c of cs)rows[g].push(await run(g,c));}process.stdout.write(JSON.stringify({hashes,rows}));})().catch(e=>{console.error(e);process.exitCode=1;});
