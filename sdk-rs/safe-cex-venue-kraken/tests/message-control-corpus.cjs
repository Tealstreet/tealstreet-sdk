const wsFile = family => path.join(src, 'exchanges/kraken/' + family + '.ts');
const classes = { 'kraken.ws-public': 'KrakenPublicWebsocket', 'kraken.ws-private': 'KrakenPrivateWebsocket', 'krakenspot.ws-public': 'KrakenSpotPublicWebsocket', 'krakenspot.ws-private': 'KrakenSpotPrivateWebsocket' };
const rawOf = new WeakMap();
function native(v) {
 if(v===undefined)return {u:true};
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {n:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}
 if(typeof v==='string')return {s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};
 if(v&&typeof v==='object'){v=rawOf.get(v)||v;return Array.isArray(v)?v.map(native):Object.fromEntries(Object.entries(v).map(([k,x])=>[k,native(x)]));}
 return v;
}
const LogSeverity=load(path.join(root,'packages/logging/src/LogSeverity.ts')).LogSeverity;
const originalError=()=>Object.assign(new Error('control failure'),{name:'ControlError'});
function failure(e,original){return e?{name:e.name,message:e.message,identity:e===original}:null;}
function traceHost(config) {
 const trace=[],original=originalError();let faulted=false;
 const event=(name,...args)=>{trace.push([name,...args.map(native)]);if(!faulted&&config.fail===name){faulted=true;throw original}};
 return {trace,original,event};
}
function dispatch(family,config) {
 const {trace,original,event}=traceHost(config);
 const r={authenticated:true,wsToken:'old',waitingChallenge:true,pendingChallenge:undefined,get isDisposed(){event('disposed');return !!config.disposed},trackLastReceived(){event('track')},recordMessageHandlerStats(feed){event('stats',feed)},parent:{log(message,severity){assert.equal(severity,LogSeverity.Error);event('log',message)}}};
 for(const key of ['authenticated','wsToken','waitingChallenge','pendingChallenge']) {let val=r[key];Object.defineProperty(r,key,{get(){return val},set(v){event('state',key,v);val=v}})}
 for(const m of ['handleTicker','handleOHLC','handleBookSnapshot','handleBookDelta','handleBook','handleFills','handleOpenOrders','handleOpenPositions','handleBalances','handleExecutions','performAuth'])r[m]=msg=>{event('invoke',m);if(m!=='performAuth')assert.equal(msg,parsed,'original parsed message identity')};
 let parsed;
 const wrap=v=>{if(v===null||typeof v!=='object')return v;const p=new Proxy(v,{get(t,k){if(typeof k==='string'){event('field',k);if(config.getFail===k)throw original;}return Reflect.get(t,k)}});rawOf.set(p,v);return p};
 const JSONbound={parse(text){event('parse',text,family==='kraken.ws-private');parsed=wrap(JSON.parse(text));return parsed},stringify(v){event('stringify',v);return JSON.stringify(rawOf.get(v)||v)}};
 const jsonFile=path.join(root,'packages/json-parse/src/jsonParse.ts');hashes['jsonParse.ts']=crypto.createHash('sha256').update(fs.readFileSync(jsonFile)).digest('hex');
 const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(jsonFile,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,JSON:JSONbound});
 const consolebound={error(...args){if(args[0].endsWith('Subscription error:'))event('consoleSubscription',...args);else {event('consoleParse',args[0],args[1]===original,args[1].name,args[1]===original?args[1].message:undefined,args[3]);}}};
 let data=config.raw===undefined?JSON.stringify(config.message):config.raw;
 if(config.dataKind==='object')data={toString(){event('toString');return config.raw||''}};
 if(config.dataKind==='missing')data={toString:undefined};
 if(config.dataKind==='null')data=null;
 const fn=method(wsFile(family),classes[family],'onMessage',{JSON:JSONbound,jsonParse:exports.jsonParse,console:consolebound,LogSeverity},r);
 let error;try{fn({data})}catch(e){error=e}
 return {family,config,trace,state:native({authenticated:r.authenticated,token:r.wsToken,waiting:r.waitingChallenge,pending:r.pendingChallenge}),failure:failure(error,original)};
}
async function auth(config) {
 const {trace,original,event}=traceHost(config);let release;
 const state={pending:config.pending===undefined?'challenge-雪':config.pending,original:'old-original',signed:'old-signed',authenticated:true};
 const r={parent:{options:{},log(message,severity){assert.equal(severity,LogSeverity.Error);event('log',message)}},emitAndLogError(message){event('emitError',message)},subscribePrivateFeeds(){event('subscribe')}};
 for(const [slot,name]of [['pending','pendingChallenge'],['original','originalChallenge'],['signed','signedChallenge'],['authenticated','authenticated']])Object.defineProperty(r,name,{get(){event('read',slot);return state[slot]},set(v){event('write',slot,v);state[slot]=v}});
 const signer={accountName:'account-雪',sign(request){event('sign',request);if(config.signSync)throw original;return new Promise((res,rej)=>{release=()=>config.signReject?rej(original):res(config.result||{wsAuthSignature:'remote-signature'})})}};
 Object.defineProperty(signer,'accountName',{get(){event('field','accountName');return 'account-雪'}});
 Object.defineProperty(signer,'sign',{get(){event('signer');return function(request){event('sign',request);if(config.signSync)throw original;return new Promise((res,rej)=>{release=()=>config.signReject?rej(original):res(new Proxy(config.result||{wsAuthSignature:'remote-signature'},{get(t,k){if(k!=='then')event('field',k);return t[k]}}))})}}});
 for(const [slot,val]of [['remote',config.remote?signer:undefined],['secret','secret-base64']])Object.defineProperty(r.parent.options,slot==='remote'?'remoteSigner':slot,{get(){event('read',slot);return val}});
 const bindings={LogSeverity,nanoid(){event('nanoid');return 'request-id'},Buffer:{from(secret,format){event('decode',secret,format);return 'decoded-secret'}},createHash(algorithm){event('hashFactory',algorithm);return {update(challenge,encoding){event('hash',challenge,algorithm,encoding);return {digest(){return 'challenge-hash'}}}}},createHmac(algorithm,secret){return {update(hash){event('hmac',secret,hash,algorithm);return {digest(format){assert.equal(format,'base64');return 'local-signature'}}}}}};
 const fn=method(wsFile('kraken.ws-private'),classes['kraken.ws-private'],'performAuth',bindings,r);
 const promise=fn();const prefix=trace.length;
 if(config.changeAwait)state.pending='changed-while-awaiting';
 if(release)release();let error;try{await promise}catch(e){error=e}
 return {config,trace,prefix,state:native(state),failure:failure(error,original)};
}
function subscriptions(config) {
 const {trace,original,event}=traceHost(config);const values={key:'K-雪',original:'original-雪',signed:undefined};
 const r={parent:{options:{}},send(wire){const payload=JSON.parse(wire);event('send',payload);if(config.mutateSend)values.key='changed';if(trace.filter(x=>x[0]==='send').length===config.sendFail)throw original}};
 Object.defineProperty(r.parent.options,'key',{get(){event('read','key');return values.key}});
 for(const slot of ['original','signed'])Object.defineProperty(r,slot+'Challenge',{get(){event('read',slot);return values[slot]}});
 const json={stringify(value){event('payload',value);return JSON.stringify(value)}};
 let error;try{method(wsFile('kraken.ws-private'),classes['kraken.ws-private'],'subscribePrivateFeeds',{JSON:json},r)()}catch(e){error=e}
 return {config,trace,failure:failure(error,original)};
}
async function refresh(family,config) {
 const {trace,original,event}=traceHost(config);let timer=config.timer||0,callback,release;let loaded={markets:true,orders:false,custom:'old'};
 const r={get ordersThrottleTimeout(){event('timeout');return timer},set ordersThrottleTimeout(v){event('setTimeoutState',v);timer=v},get isDisposed(){event('disposed');return !!config.disposed},parent:{fetchOrders(){event('fetch');if(config.fetchSync)throw original;return new Promise((res,rej)=>{release=()=>config.reject?rej(original):res(['order-marker'])})},get store(){return {get loaded(){event('loaded');return loaded},update(update){event('update',update)}}},async fetchAndSetOrders(){event('fetchSet');if(config.reject)throw original}}};
 const bindings={clearTimeout(value){event('clear',value)},setTimeout(fn,delay){event('schedule',family,delay);callback=fn;return 73}};
 if(family==='fetch') {
  const fn=method(wsFile('kraken.ws-private'),classes['kraken.ws-private'],'fetchThrottledOrders',{},r);
  const promise=fn();const prefix=trace.length;loaded={markets:false,orders:false,custom:'current'};if(release)release();let error;try{await promise}catch(e){error=e}
  return {family,config,trace,prefix,failure:failure(error,original)};
 }
 r.fetchThrottledOrders=()=>{};
 let error;try{method(wsFile(family==='futures'?'kraken.ws-private':'krakenspot.ws-private'),classes[family==='futures'?'kraken.ws-private':'krakenspot.ws-private'],'throttledFetchOrders',bindings,r)();if(family==='spot'&&callback&&config.invoke)await callback()}catch(e){error=e}
 return {family,config,trace,failure:failure(error,original)};
}
async function messageControlCorpus(){
 const dispatches=[];
 const families=Object.keys(classes);
 const messages=[{},null,{feed:'ticker',channel:'book'},{channel:'ticker',type:'snapshot'},{channel:'book',type:'update'},{feed:'book_snapshot'},{feed:'book'},{channel:'ohlc'},{channel:'heartbeat'},{channel:'unknown-雪'},{event:'error',message:''},{event:'challenge',message:'challenge-雪'},{feed:'challenge',message:17.25},{event:'challenge',message:false},{feed:'info',message:'authentication failed'},{feed:'info',message:'other'},{feed:'error',message:'bad-key'},{feed:'subscribed'},{feed:'open_orders_snapshot'},{feed:'open_positions_snapshot'},{feed:'account_balances_snapshot'},{channel:'executions',type:'update'},{channel:'balances',type:'snapshot'},{method:'subscribe',success:false,error:'invalid token'},{method:'subscribe',success:false,error:null},{method:'subscribe',success:true},{method:'unsubscribe',success:false},{channel:17.25,type:'update'},{feed:'',channel:'ticker'},{feed:'fills'}];
 for(const family of families) {
  for(const message of messages)dispatches.push(dispatch(family,{message}));
  for(const config of [{disposed:true,message:{channel:'ticker'}},{raw:'invalid'},{dataKind:'object',raw:'{"channel":"ticker"}'},{dataKind:'object',raw:''},{dataKind:'missing'},{dataKind:'null'},{message:{channel:'ticker'},fail:'track'},{message:{channel:'ticker'},fail:'stats'},{message:{channel:'ticker'},fail:'invoke'},{message:{method:'subscribe',error:'token'},fail:'log'},{message:{channel:'ticker'},getFail:'channel'},{message:{feed:'error',message:{$units:[55296,38634]}}}])dispatches.push(dispatch(family,config));
 }
 const auths=[];for(const config of [{},{pending:''},{pending:null},{pending:false},{pending:0},{remote:true},{remote:true,changeAwait:true},{remote:true,result:{error:'offline'}},{remote:true,result:{error:''}},{remote:true,result:{}},{remote:true,signReject:true},{remote:true,signSync:true},{fail:'hash'},{fail:'decode'},{fail:'hmac'},{fail:'subscribe'},{remote:true,fail:'subscribe'},{remote:true,result:{error:'offline'},fail:'emitError'},{fail:'hashFactory'},{remote:true,result:{error:17.25}}])auths.push(await auth(config));
 const subs=[];for(const config of [{},{mutateSend:true},{sendFail:1},{sendFail:2},{sendFail:3},{sendFail:4},{fail:'read'},{fail:'payload'}])subs.push(subscriptions(config));
 const refreshes=[];for(const family of ['futures','spot'])for(const config of [{},{timer:37},{timer:-1},{timer:37,fail:'clear'},{fail:'schedule'},{fail:'setTimeoutState'},{invoke:true},{invoke:true,reject:true}])refreshes.push(await refresh(family,config));
 for(const config of [{},{disposed:true},{reject:true},{fetchSync:true},{fail:'loaded'},{fail:'update'},{fail:'setTimeoutState'},{fail:'disposed'}])refreshes.push(await refresh('fetch',config));
 return {dispatches,auths,subs,refreshes};
}

messageControlCorpus;
