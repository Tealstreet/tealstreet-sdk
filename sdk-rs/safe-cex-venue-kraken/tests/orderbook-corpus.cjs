const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
const getKeyFile=path.join(src,'utils/get-key.ts'),getKeyText=fs.readFileSync(getKeyFile,'utf8');hashes['get-key.ts']=crypto.createHash('sha256').update(getKeyText).digest('hex');
const ast=ts.createSourceFile(getKeyFile,getKeyText,ts.ScriptTarget.Latest,true),decl=ast.statements.filter(ts.isVariableStatement).flatMap(x=>[...x.declarationList.declarations]).find(d=>d.name.getText(ast)==='v');assert.ok(decl);
const lodash=name=>require(path.join(root,'node_modules/lodash',name));
const getV=vm.runInNewContext(ts.transpileModule('function make(){return ('+decl.initializer.getText(ast)+')}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';make()', {s:lodash('memoize')(lodash('snakeCase')),c:lodash('memoize')(lodash('camelCase'))});
const underlying=new WeakMap();
function native(v){if(typeof v==='function')return {function:{s:units(v.name)}};if(v===undefined)return {u:true};if(typeof v==='number')return {n:capture(v).number};if(typeof v==='string')return {s:units(v)};if(v&&typeof v==='object'){v=underlying.get(v)||v;return Array.isArray(v)?v.map(native):Object.fromEntries(Object.entries(v).map(([k,x])=>[k,native(x)]))}return v}
function value(v){if(v&&typeof v==='object'&&Object.hasOwn(v,'$number'))return {nan:NaN,inf:Infinity,negzero:-0}[v.$number];if(v&&typeof v==='object'&&Object.hasOwn(v,'$units'))return String.fromCharCode(...v.$units);if(v&&typeof v==='object'&&Object.hasOwn(v,'$undefined'))return undefined;return v}
function setup(config){const trace=[],original=Object.assign(new Error('book failure'),{name:'BookError'});let failed=false;const event=(name,...args)=>{trace.push([name,...args.map(native)]);if(config.fail===name&&!failed){failed=true;throw original}};return {trace,original,event};}
function bookCase(family,operation,config){
 const {trace,original,event}=setup(config);const symbol=value(config.symbol===undefined?'PF_雪':config.symbol);
 let cache,cacheReads=0,cacheWrites=0;const levels=config.levels||[{price:'17.25tail',qty:'19.5'},{price:'23.75',qty:29.125},{price:'17.25',qty:31.5}];
 const maps=new Set();
 function map(side,initial=[]){const m=new Map(initial.map(([p,q])=>[value(p),value(q)]));maps.add(m);return new Proxy(m,{get(t,k){if(k==='set')return (price,qty)=>{event('set',side,price,qty);t.set(price,qty);return t};if(k==='delete')return price=>{event('delete',side,price);return t.delete(price)};if(k==='entries')return ()=>{event('entries',side);return t.entries()};return Reflect.get(t,k,t)}})}
 function makeCache(initial=false){return {bids:map('bids',initial?(config.cacheBids||[[5.5,7.25],[17.25,11.5]]):[]),asks:map('asks',initial?(config.cacheAsks||[[41.25,43.5],[37.75,47.125]]):[])}}
 cache=config.noCache?undefined:makeCache(true);
 const items=rows=>(rows||[]).map(row=>{if(row===null)return null;const raw=Object.fromEntries(Object.entries(row).map(([k,v])=>[k,value(v)]));const p=new Proxy(raw,{get(t,k){if(typeof k==='string')event('field',k,'direct');if(config.recacheOnQty&&k==='qty')cache=makeCache(true);return t[k]}});underlying.set(p,raw);return p});
 const rawBids=config.bids===undefined?items(levels):Array.isArray(config.bids)?items(config.bids):value(config.bids),rawAsks=config.asks===undefined?items([{price:37.75,qty:41.125}]):Array.isArray(config.asks)?items(config.asks):value(config.asks);
 const source={product_id:symbol,bids:rawBids,asks:rawAsks,side:config.side===undefined?'buy':value(config.side),price:value(config.price===undefined?'17.25tail':config.price),qty:value(config.qty===undefined?'13.125tail':config.qty)};
 if(config.alias){source.productId=source.product_id;delete source.product_id;}
 const item=new Proxy(source,{get(t,k){if(typeof k==='string')event('field',k,'direct');if(config.recacheOnBids&&k==='bids')cache=makeCache(true);return t[k]}});underlying.set(item,source);
 const callbacks=config.noHandlers?[]:[0,1].map(index=>book=>{
  event('callback',index,book);assert.notEqual(book,priorBook,'fresh outer book for each callback');if(priorBook)assert.equal(book.asks,priorBook.asks,'shared asks array');priorBook=book;
  if(index===0&&config.mutateLevels&&book.bids.length)book.bids[0].amount=59.25;
  if(index===0&&config.replaceBids)book.bids=[];
  if(index===0&&config.deleteSecond)delete callbacks[1];
  if(index===0&&config.appendCallback)callbacks.push(()=>{throw Error('forEach must use initial length')});
  if(config.retainLevel){if(index===0&&book.bids.length)retained=book.bids.splice(0,1)[0];if(index===1&&retained)retained.amount=91.25;event('retained',retained,book.bids.includes(retained));}
 });let priorBook,retained;
 const handlers=new Proxy(Object.create(null),{get(t,k){event('handlers',String(k));return callbacks}});
 const caches=new Proxy(Object.create(null),{get(t,k){event('cache',String(k));cacheReads++;return cache},set(t,k,v){event('reset',String(k));cacheWrites++;cache={bids:map('bids'),asks:map('asks')};return true}});
 const r={orderBookHandlers:handlers,orderBookCache:caches,recordMessageHandlerStats(feed){event('stats',feed);if(config.clearStats)callbacks.length=0},emitOrderBook(sym){event('emit',sym)},parent:{store:{markets:{find(fn){event('market',symbol);const market={symbol,contractSize:value(config.contractSize===undefined?2.75:config.contractSize)};return config.noMarket?undefined:fn(market)?market:undefined}}}}};
 const file=path.join(src,'exchanges/kraken',family==='Futures'?'kraken.ws-public.ts':'krakenspot.ws-public.ts');
 const binding={v(object,key){event('field',key,'v');if(config.recacheOnBids&&key==='bids')cache=makeCache(true);return getV(source,key)},Map,parseFloat(v){event('parse');return Number.parseFloat(v)}};
 const fn=method(file,family==='Futures'?'KrakenPublicWebsocket':'KrakenSpotPublicWebsocket',operation==='snapshot'?'handleBookSnapshot':operation==='delta'?'handleBookDelta':'emitOrderBook',binding,r);
 let error;try{if(operation==='emit')fn(symbol);else if(family==='Futures')fn(source);else fn(symbol,item)}catch(e){error=e}
 const finalCache=cache?{bids:[...cache.bids.entries()],asks:[...cache.asks.entries()]}:null;
 // Reading final cache is evidence capture, outside the source method trace.
 if(cache)trace.splice(-2);
 return {family,operation,config,trace,cache:native(finalCache),retained:native(retained),failure:error?{name:error.name,identity:error===original}:null};
}
function listenerCase(family,config){
 const {trace,original,event}=setup(config);let connected=!!config.connected;const symbol=value(config.symbol===undefined?'BTC/雪':config.symbol),optsRaw={symbol,interval:value(config.interval===undefined?'1m':config.interval)};
 const opts=new Proxy(optsRaw,{get(t,k){event('field',k,'direct');return t[k]}});underlying.set(opts,optsRaw);
 const callback=()=>{},other=()=>{};let polls=[],firstPoll,firstFrame;const maps={book:Object.create(null),ohlc:Object.create(null)};
 const key=family==='ohlc'?`${symbol}::${optsRaw.interval}`:String(symbol);
 maps[family==='ohlc'?'ohlc':'book'][key]=config.existing?[callback,other,callback]:[];
 function handlerMap(feed){return new Proxy(maps[feed],{get(t,k){event('getHandlers',feed,String(k));return t[k]},set(t,k,v){event('setHandlers',feed,String(k),v.map(cb=>cb===callback?'same':'other'));t[k]=v;return true}})}
 const r={orderBookHandlers:handlerMap('book'),ohlcvHandlers:handlerMap('ohlc'),get isConnected(){event('connected');return connected},send(wire){event('send',JSON.parse(wire));if(config.sendFail)throw original},clearFeedStatsIfIdle(feed,handlers){assert.equal(handlers,feed==='book'?r.orderBookHandlers:r.ohlcvHandlers);event('clearIdle',feed);if(config.clearDeletes)delete maps[feed][key];if(config.clearAdds)maps[feed][key]=[other]}};
 const bindings={INTERVAL:spotTypes.INTERVAL,JSON:{stringify(frame){event('frame',frame);if(family==='ohlc'&&config.interval==='constructor')assert.equal(frame.params.interval,spotTypes.INTERVAL.constructor,'inherited interval identity');if(family==='Futures'){if(firstFrame)assert.equal(firstFrame,frame,'original Futures frame');firstFrame=frame}return JSON.stringify(frame)}},setTimeout(fn,delay){if(firstPoll)assert.equal(fn,firstPoll,'same recurring callback');firstPoll=fn;event('timer',delay);polls.push(fn)},console:{error(prefix,e){event('sendError',prefix,e===original)}}};
 const file=path.join(src,'exchanges/kraken',family==='Futures'?'kraken.ws-public.ts':'krakenspot.ws-public.ts'),name=family==='ohlc'?'listenOHLCV':'listenOrderBook';
 const fn=method(file,family==='Futures'?'KrakenPublicWebsocket':'KrakenSpotPublicWebsocket',name,bindings,r);
 let disposer,error;try{disposer=fn(family==='ohlc'?opts:symbol,callback)}catch(e){error=e}
 const registered=!!disposer;const checkpoints=[trace.length];
 for(const now of config.polls||[]){connected=now;const fn=polls.shift();if(fn){try{fn()}catch(e){error=e}}checkpoints.push(trace.length)}
 if(config.mutateOpts)optsRaw.symbol='CHANGED/雪';
 connected=config.disposeConnected===undefined?connected:config.disposeConnected;
 for(let i=0;i<(config.dispose===undefined?1:config.dispose);i++){if(disposer){try{disposer()}catch(e){error=e}}checkpoints.push(trace.length)}
 return {family,config,trace,checkpoints,registered,handlers:Object.entries(maps[family==='ohlc'?'ohlc':'book']).map(([k,v])=>({key:units(k),callbacks:v.map(cb=>cb===callback?'same':'other')})),failure:error?{name:error.name,identity:error===original}:null};
}
function orderbookCorpus(){
 const books=[];
 const snapshotConfigs=[{}, {noHandlers:true},{symbol:''},{symbol:{$units:[55296,38634]}},{alias:true},{clearStats:true},{bids:null,asks:false},{bids:'ab',asks:[]},{bids:[null]},{bids:[{price:'-0',qty:0},{price:'0',qty:-0},{price:'NaN',qty:17.25},{price:'Infinity',qty:19.5},{price:'23',qty:-3},{price:'25',qty:'NaN'}]},{recacheOnBids:true},{recacheOnQty:true},{levels:[{price:17.25,qty:true},{price:19.5,qty:{$number:'nan'}},{price:23.75,qty:{$number:'inf'}}]},{fail:'handlers'},{fail:'reset'},{fail:'set'},{fail:'emit'}];
 const deltaConfigs=[{}, {noHandlers:true},{noCache:true},{clearStats:true},{symbol:''},{qty:0},{qty:'-0'},{qty:-17.25},{qty:'NaN'},{qty:{$number:'inf'}},{side:'Buy'},{side:null},{price:'NaN'},{price:'-0'},{bids:[{price:5.5,qty:0},{price:17.25,qty:-19.5}],asks:[{price:37.75,qty:'NaN'}]},{recacheOnQty:true},{recacheOnBids:true},{bids:null,asks:false},{bids:'ab',asks:[]},{bids:[null]},{fail:'stats'},{fail:'cache'},{fail:'set'},{fail:'delete',qty:0,bids:[{price:5.5,qty:0}]},{fail:'emit'}];
 const emitConfigs=[{}, {noHandlers:true},{noCache:true},{noMarket:true},{contractSize:0},{contractSize:-0},{contractSize:false},{contractSize:'3.25'},{contractSize:'invalid'},{contractSize:{$number:'inf'}},{contractSize:{$number:'nan'}},{mutateLevels:true},{retainLevel:true},{replaceBids:true},{deleteSecond:true},{appendCallback:true},{cacheBids:[[{$number:'negzero'},7.25],[0,17.25],[{$number:'nan'},19.5],[{$number:'nan'},23.75],[{$number:'inf'},29.125]],cacheAsks:[]},{cacheBids:Array.from({length:103},(_,i)=>[i+0.25,i+0.5]),cacheAsks:Array.from({length:102},(_,i)=>[i+0.75,i+1.5])},{fail:'entries'},{fail:'callback'},{fail:'market'}];
 for(const family of ['Futures','Spot'])for(const [operation,configs]of [['snapshot',snapshotConfigs],['delta',deltaConfigs],['emit',emitConfigs]])for(const config of configs)books.push(bookCase(family,operation,config));
 const listeners=[];
 const configs=[{connected:true},{},{polls:[false,true]},{connected:true,existing:true},{connected:true,dispose:2},{connected:true,sendFail:true},{polls:[true],sendFail:true},{polls:[false,false],dispose:0},{connected:true,clearDeletes:true},{connected:true,clearAdds:true},{connected:true,disposeConnected:false},{connected:false,disposeConnected:true},{connected:true,fail:'clearIdle'},{connected:true,fail:'timer'},{connected:true,symbol:{$units:[55296,38634]}},{connected:true,mutateOpts:true},{connected:true,interval:'3m'},{connected:true,interval:'constructor'}];
 for(const family of ['Futures','Spot','ohlc'])for(const config of configs)listeners.push(listenerCase(family,config));
 return {books,listeners};
}
orderbookCorpus;
