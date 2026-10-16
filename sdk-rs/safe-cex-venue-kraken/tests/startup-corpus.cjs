function flags(){return {balance:true,orders:false,markets:false,tickers:true,positions:false}}
function changedFlags(){return {balance:false,orders:true,markets:false,tickers:false,positions:true}}
const pump=async()=>{await Promise.resolve();await Promise.resolve();await Promise.resolve();await Promise.resolve()};
async function startCase(family,config){
 const trace=[],release={};const original=new Error('startup failure');original.name='StartupError';
 const markets=[{tag:'M',marker:17.25}],tickers=[{tag:'T',marker:19.5}];
 const r={isDisposed:!!config.disposed,isPublicDataOnly:!!config.only,store:{loaded:flags()}};
 const state=()=>({disposed:r.isDisposed,only:r.isPublicDataOnly,loaded:{...r.store.loaded},public:r.publicWebsocket?.id??null,private:r.privateWebsocket?.id??null});
 function effect(stage){if(config.disposeAt===stage)r.isDisposed=true;if(config.onlyAt===stage)r.isPublicDataOnly=true;if(config.loadedAt===stage)r.store.loaded=changedFlags()}
 function socket(kind,id){return {id,connectAndSubscribe(){trace.push({kind:'connect',socket:kind,id,state:state()});effect(kind.endsWith('Public')?'public-connect':'private-connect');if(config.replaceConnect&&kind.endsWith('Public'))r.publicWebsocket=socket(kind,41);if(config.failAt===(kind.endsWith('Public')?'public-connect':'private-connect'))throw original}}}
 const ctor=kind=>function(parent){trace.push({kind:'create',socket:kind,parentIdentity:parent===r,state:state()});effect(kind.endsWith('Public')?'public-create':'private-create');if(config.failAt===(kind.endsWith('Public')?'public-create':'private-create'))throw original;return socket(kind,kind.endsWith('Public')?31:37)};
 if(config.existing){r.publicWebsocket=socket(family+'Public',7);r.privateWebsocket=socket(family+'Private',9)}
 for(const stage of ['init','markets','tickers']){
  const name={init:'initExternalPublicData',markets:'fetchMarkets',tickers:'fetchTickers'}[stage];
  r[name]=()=>{trace.push({kind:stage,state:state()});effect(stage+'-call');if(config.failAt===stage+'-call')throw original;return new Promise((resolve,reject)=>{release[stage]=()=>{effect(stage+'-resolve');if(config.failAt===stage+'-resolve')reject(original);else resolve(stage==='init'?!!config.external:stage==='markets'?markets:tickers)}})};
 }
 r.store.update=update=>{const stage=update.markets?'markets-update':update.tickers?'tickers-update':'loaded-update';trace.push({kind:stage,loaded:{...update.loaded},batchIdentity:stage==='markets-update'?update.markets===markets:stage==='tickers-update'?update.tickers===tickers:null,marker:stage==='markets-update'?capture(update.markets[0].marker):stage==='tickers-update'?capture(update.tickers[0].marker):null,state:state()});r.store.loaded={...update.loaded};effect(stage);if(config.failAt===stage)throw original};
 r.tick=count=>{trace.push({kind:'tick',count:capture(count),state:state()});effect('tick');if(config.failAt==='tick')throw original;return new Promise(()=>{})};
 const fn=method(family==='Futures'?futuresFile:spotFile,family==='Futures'?'KrakenExchange':'KrakenSpotExchange','start',{KrakenPublicWebsocket:ctor('FuturesPublic'),KrakenPrivateWebsocket:ctor('FuturesPrivate'),KrakenSpotPublicWebsocket:ctor('SpotPublic'),KrakenSpotPrivateWebsocket:ctor('SpotPrivate')},r);
 let done=false,failure=null;fn().then(()=>{done=true},e=>{done=true;failure={name:e.name,message:e.message,identity:e===original}});
 await pump();const checkpoints=[{state:state(),events:trace.length,done}];
 for(const stage of ['init','markets','tickers']){if(release[stage])release[stage]();await pump();checkpoints.push({state:state(),events:trace.length,done})}
 assert.equal(done,true,'source startup settles without awaiting tick');
 return {config,trace,checkpoints,state:state(),failure};
}
async function ordersSetCase(config){
 const trace=[];let release;const original=new Error('orders snapshot failure');original.name='SnapshotError';
 const orders=[{id:'O',marker:23.75}],store={loaded:flags()};
 const r={store,fetchOrders(){trace.push({kind:'fetch',loaded:{...store.loaded}});if(config.syncFail)throw original;return new Promise((resolve,reject)=>{release=()=>config.fetchFail?reject(original):resolve(orders)})}};
 store.addOrUpdateOrdersFull=value=>{trace.push({kind:'add',identity:value===orders,marker:capture(value[0].marker),loaded:{...store.loaded}});if(config.mutateBatch)value[0].marker=29.125;if(config.loadedAtAdd)store.loaded=changedFlags();if(config.addFail)throw original};
 store.update=value=>{trace.push({kind:'update',loaded:{...value.loaded}});store.loaded={...value.loaded};if(config.updateFail)throw original};
 const fn=method(spotFile,'KrakenSpotExchange','fetchAndSetOrders',{},r);const promise=fn();const prefix=trace.length;
 if(config.loadedAwait)store.loaded=changedFlags();if(release)release();let failure=null,result=null;
 try{const value=await promise;result={identity:value===orders,marker:capture(value[0].marker)}}catch(e){failure={name:e.name,message:e.message,identity:e===original}}
 return {config,trace,prefix,loaded:store.loaded,result,failure};
}
async function startupCorpus(){
 const specs=[{}, {only:true},{external:true},{external:true,only:true},{existing:true},{external:true,existing:true},{disposed:true},{external:true,disposed:true},
  ...['init-resolve','markets-resolve','tickers-resolve','markets-update','tickers-update','public-create','public-connect','private-create','private-connect'].map(disposeAt=>({disposeAt})),
  ...['init-resolve','markets-update','public-connect','private-connect'].map(onlyAt=>({onlyAt})),
  ...['markets-resolve','markets-update','tickers-resolve','tickers-update','private-connect'].map(loadedAt=>({loadedAt})),
  ...['init-call','init-resolve','markets-call','markets-resolve','markets-update','tickers-call','tickers-resolve','tickers-update','public-create','public-connect','private-create','private-connect','tick'].map(failAt=>({failAt})),
  {only:true,failAt:'loaded-update'},{replaceConnect:true}];
 const starts={};for(const family of ['Futures','Spot']){starts[family]=[];for(const config of specs)starts[family].push(await startCase(family,config))}
 const orders=[];for(const config of [{},{loadedAwait:true},{loadedAtAdd:true},{mutateBatch:true},{syncFail:true},{fetchFail:true},{addFail:true},{updateFail:true},{loadedAtAdd:true,addFail:true}])orders.push(await ordersSetCase(config));
 return {starts,orders};
}
