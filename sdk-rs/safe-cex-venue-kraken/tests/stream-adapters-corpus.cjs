const publicFile=path.join(src,'exchanges/kraken/kraken.ws-public.ts');
const candleValue={timestamp:17.25,open:19.5,high:23.75,low:29.125,close:31.5,volume:37.25};
const bookValue={bids:[{price:17.25,amount:19.5,total:23.75}],asks:[]};
const optionsValue={symbol:'BTC/雪',interval:'12h',from:-0,to:1770900000123.75,limit:17.25};
function streamCase(family,config){
 const trace=[];const original=new Error('stream failure');original.name='StreamError';let first=true;
 const r={publicWebsocket:undefined};
 const callback=(value)=>{trace.push({kind:'callback',value:capture(value)});if(config.callbackFail&&first)throw original};
 const expectedValue=family==='spot-ohlcv'?candleValue:bookValue;
 let latestDisposer;
 function socket(id){const disposer=()=>{trace.push({kind:'unsubscribe',id});if(config.unsubscribeFail&&first)throw original};return {id,disposer,
  connectAndSubscribe(){trace.push({kind:'connect',id});if(config.replaceConnect&&first)r.publicWebsocket=socket(13);if(config.removeConnect&&first)r.publicWebsocket=undefined;if(config.connectFail&&first)throw original},
  listenOrderBook(symbol,cb){trace.push({kind:'listen',id,symbol,callbackIdentity:cb===callback});if(config.listenFail&&first)throw original;cb(expectedValue);latestDisposer=disposer;return disposer},
  listenOHLCV(options,cb){trace.push({kind:'listen',id,options:capture(options),optionsIdentity:options===optionsValue,callbackIdentity:cb===callback});if(config.listenFail&&first)throw original;cb(expectedValue);latestDisposer=disposer;return disposer},
 }}
 function createCtor(venue){return function Ctor(parent){trace.push({kind:'create',venue,parentIdentity:parent===r});if(config.replaceCreate&&first)r.publicWebsocket=socket(13);if(config.createFail&&first)throw original;return socket(11)}}
 if(config.existing)r.publicWebsocket=socket(7);
 const fn=method(family==='futures-book'?futuresFile:spotFile,family==='futures-book'?'KrakenExchange':'KrakenSpotExchange',family==='spot-ohlcv'?'listenOHLCV':'listenOrderBook',{KrakenPublicWebsocket:createCtor('futures'),KrakenSpotPublicWebsocket:createCtor('spot')},r);
 const outcomes=[];
 for(let attempt=0;attempt<2;attempt++){
  try {const returned=family==='spot-ohlcv'?fn(optionsValue,callback):fn('BTC/雪',callback);const identity=returned===latestDisposer;returned();returned();outcomes.push({identity,slot:r.publicWebsocket.id})}
  catch(e){outcomes.push({failure:{name:e.name,message:e.message,identity:e===original},slot:r.publicWebsocket?.id??null})}
  first=false;
 }
 return {config,trace,outcomes};
}
function streamCorpus(){
 const specs=[{},{existing:true},{createFail:true},{connectFail:true},{listenFail:true},{callbackFail:true},{unsubscribeFail:true},{replaceConnect:true},{replaceConnect:true,connectFail:true},{existing:true,listenFail:true},{replaceCreate:true},{replaceCreate:true,createFail:true},{removeConnect:true}];
 const streams=Object.fromEntries(['futures-book','spot-book','spot-ohlcv'].map(family=>[family,specs.map(config=>streamCase(family,config))]));
 const noops=[];
 for(const value of [undefined,null,false,0,-0,NaN,Infinity,'',[],{},'😀','\ud800']){
  let callbackCalls=0;const r={send(){throw Error('no send allowed')},parent:{get store(){throw Error('no Store access allowed')}}};
  const listen=method(publicFile,'KrakenPublicWebsocket','listenOHLCV',{},r),handle=method(publicFile,'KrakenPublicWebsocket','handleOHLC',{},r);
  const callback=()=>{callbackCalls++};const a=listen(value,callback),b=listen(value,callback);const first=a(),second=a(),third=b(),handled=handle(value);
  noops.push({input:typeof value==='string'?{units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))}:capture(value),distinct:a!==b,returns:[first,second,third,handled].map(capture),callbackCalls});
 }
 return {streams,noops};
}
