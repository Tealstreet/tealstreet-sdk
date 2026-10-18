const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs'),root=path.resolve(__dirname,'../../..');
const compile=source=>ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
function load(file){const out={};vm.runInNewContext(compile(fs.readFileSync(path.join(root,file),'utf8')),{exports:out,require:runtime});return out;}
const {jsonParse}=load('packages/json-parse/src/jsonParse.ts');
const {v}=load('packages/safe-cex/src/utils/get-key.ts');
const typesFile=path.join(root,'packages/safe-cex/src/exchanges/woo/woo.types.ts');const types=ts.createSourceFile(typesFile,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true);
const urls=types.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.getText(types)==='BASE_WS_URL');assert(urls);
const bindings={};vm.runInNewContext(compile(`this.urls=${urls.initializer.getText(types)}`),bindings);
function classes(Base,extra={}){
 const context={BaseWebSocket:Base,BASE_WS_URL:bindings.urls,ENABLE_RECONNECTING_WEBSOCKET:false,flatten:runtime('lodash/flatten'),jsonParse,v,performance:{now:()=>79},virtualClock:{getCurrentTimeMs:()=>73},createHmac:runtime('create-hmac'),...extra};
 for(const [file,name]of[['woo.ws-public.ts','WooPublicWebsocket'],['woo.ws-private.ts','WooPrivateWebscoket']]){
  const p=path.join(root,'packages/safe-cex/src/exchanges/woo',file),ast=ts.createSourceFile(p,fs.readFileSync(p,'utf8'),ts.ScriptTarget.Latest,true),c=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===name);assert(c);
  vm.runInNewContext(compile(c.getText(ast).replace(/^export /,`this.${name}=`)),context);
 }
 return context;
}
function layout(r,baseKeys){const keys=Object.keys(r).filter(k=>!baseKeys.includes(k));for(const key of keys){const descriptor=Object.getOwnPropertyDescriptor(r,key);assert(descriptor.enumerable&&descriptor.configurable&&descriptor.writable);}
 return {keys,functions:keys.filter(k=>typeof r[k]==='function').map(k=>[r[k].name,r[k].length,Object.getPrototypeOf(r[k]).constructor.name==='AsyncFunction'])};}
function setup(extra={}){const sent=[],trace=[];class Base{
 constructor(parent,...rest){this.parent=parent;this.isDisposed=false;this.baseArguments=[parent,...rest];}
 get store(){return this.parent.store;}
 send(text){sent.push([false,text]);}sendWithLog(text){sent.push([true,text]);}
 resetOrderbook(){trace.push('resetOrderbook');}trackLastReceived(){trace.push('trackLastReceived');}emitWsDiagnosticReceive(){trace.push('emitWsDiagnosticReceive');}recordMessageHandlerStats(){trace.push('recordMessageHandlerStats');}handlePongEvent(){trace.push('handlePongEvent');}
 }
 return Object.assign(classes(Base,extra),{sent,trace});}
async function run(kind){
 if(kind==='layout'){
  const c=setup(),parent={},a=new c.WooPublicWebsocket(parent,83,'extra'),b=new c.WooPublicWebsocket(parent,83,'extra'),p=new c.WooPrivateWebscoket(parent,83,'extra'),q=new c.WooPrivateWebscoket(parent,83,'extra');
  const base=['parent','isDisposed','baseArguments'],publicLayout=layout(a,base),privateLayout=layout(p,base);
  assert.equal(publicLayout.keys.length,14);assert.equal(privateLayout.keys.length,8);for(const [x,y]of[[a,b],[p,q]]){for(const key of Object.keys(x).filter(k=>!base.includes(k)))assert.notEqual(x[key],y[key]);assert.equal(x.baseArguments.length,3);assert.equal(x.baseArguments[0],parent);}
  const returns=[];for(const [index,[key,method]]of[['ping','handlePingEvent'],['pong','handlePongEvent'],['tickers','handleTickersStreamEvents'],['bbos','handleBBOStreamEvents'],['markprices','handleMarkPricesStreamEvents']].entries()){
   const wrapper=a.messageHandlers[key][0],other=b.messageHandlers[key][0],payload={};assert.notEqual(wrapper,other);assert.notEqual(a.messageHandlers[key],b.messageHandlers[key]);assert.equal(wrapper.name,'');assert.equal(wrapper.length,index>1?1:0);let count=0;
   const replacement=function(...args){assert.equal(this,a);assert.equal(args.length,index>1?1:0);if(index>1)assert.equal(args[0],payload);count++;return 89+index;};
   let input={data:payload};if(key==='bbos'){a[method]=()=>{throw Error('stale handler');};input={get data(){a[method]=replacement;return payload;}};}else a[method]=replacement;
   returns.push(wrapper.call({},input));assert.equal(count,1);
  }
  return {public:publicLayout,private:privateLayout,returns};
 }
 if(kind==='payload-alias'){
  const c=setup();c.normalizeSymbol=load('packages/safe-cex/src/exchanges/woo/woo.utils.ts').normalizeSymbol;
  const ticker={last:3},store={tickersBySymbol:{BTCUSDT:ticker},updateTicker:(row,patch)=>Object.assign(row,patch)};
  const r=new c.WooPublicWebsocket({store}),input=[{symbol:'PERP_BTC_USDT',close:31,amount:37,volume:41}];
  input[0].close=59;r.handleTickersStreamEvents(input);assert.equal(ticker.last,59);
  return {last:ticker.last};
 }
 if(kind==='public'){
  let bookCalls=0;const symbolCalls=[],sentinel=Error('reverse symbol sentinel');const c=setup({reverseSymbol:symbol=>{symbolCalls.push(symbol);throw sentinel;},updateOrderBookByPrice:(book,side,price,amount)=>{assert.equal(book,expectedBook);assert.equal(side,'bids');assert.equal(price,101);assert.equal(amount,103);bookCalls++;}});
  const ticker={last:3,quoteVolume:20,volume:10,bid:1,ask:2,mark:4};const store={tickersBySymbol:{BTCUSDT:ticker},updateTicker:(row,patch)=>{assert.equal(row,ticker);Object.assign(row,patch);}};
  const r=new c.WooPublicWebsocket({options:{applicationId:'app',testnet:false},store});const listeners=[];let url;const ws={addEventListener(event,callback){assert.equal(callback,r[{open:'onOpen',message:'onMessage',close:'onClose'}[event]]);listeners.push(event);}};r.onClose=()=>{};r.createWebsocket=value=>{url=value;return ws;};r.connectAndSubscribe.call({});assert.equal(r.ws,ws);r.onOpen();assert.equal(r.pingAt,79);
  c.normalizeSymbol=load('packages/safe-cex/src/exchanges/woo/woo.utils.ts').normalizeSymbol;
  r.handleTickersStreamEvents([{symbol:'PERP_BTC_USDT',close:31,amount:37,volume:41}]);r.handleBBOStreamEvents([{symbol:'PERP_BTC_USDT',bid:43,ask:47}]);r.handleMarkPricesStreamEvents([{symbol:'PERP_BTC_USDT',price:53}]);
  r.onMessage({data:'{"event":"ping"}'});
  const expectedBook={};r.processOrderBookUpdate(expectedBook,{data:{bids:[[101,103]],asks:[]}});
  for(const call of[()=>r.listenOHLCV({symbol:'BTC'},()=>{}),()=>r.listenOrderBook('ETH',()=>{})]){assert.throws(call,e=>e===sentinel);}
  return {ticker:[ticker.last,ticker.quoteVolume,ticker.volume,ticker.bid,ticker.ask,ticker.mark],url,listeners,sent:c.sent,symbolCalls,bookCalls};
 }
 if(kind==='private'){
  const c=setup(),maps=[];let limitInput,algoInput;const parent={options:{key:'fixture-key',secret:'fixture-secret',applicationId:'app',testnet:false},mapLimitOrder(data){assert.equal(this,parent);assert.equal(data,limitInput);maps.push('limit');return undefined;},mapAlgoOrder(data){assert.equal(this,parent);assert.equal(data,algoInput);maps.push('algo');return [undefined];}};
  const r=new c.WooPrivateWebscoket(parent);let url,listeners=0;const ws={addEventListener(){listeners++;}};r.onClose=()=>{};r.createWebsocket=value=>{url=value;return ws;};r.connectAndSubscribe();
  assert.equal(Object.getPrototypeOf(r.onOpen).constructor.name,'AsyncFunction');const promise=r.onOpen();assert.equal(typeof promise.then,'function');await promise;
  limitInput={};algoInput={algoStatus:'NEW'};r.handleExecutionReport(limitInput);r.handleAlgoExecutionReport(algoInput);const timestamp=r.parseTimestampToSeconds(1500000000123);r.ping();assert.equal(r.pingAt,79);r.subscribe();
  const sentinel=Error('disposed getter sentinel');Object.defineProperty(r,'isDisposed',{configurable:true,get(){throw sentinel;}});const rejected=r.onOpen();assert.equal(typeof rejected.then,'function');let asyncError=false;try{await rejected;}catch(e){assert.equal(e,sentinel);asyncError=true;}Object.defineProperty(r,'isDisposed',{configurable:true,writable:true,value:false});
  let subscribed=0;r.subscribe=()=>{subscribed++;};r.onMessage({data:'{"event":"auth","success":true}'});
  return {sent:c.sent,maps,timestamp,subscribed,asyncError,url,listeners};
 }
 if(kind==='heartbeat-audit'){
  const results=[];
  for(const [channel,method] of [['public','ping'],['private','ping'],['public','handlePingEvent']]){
   for(const mode of ['success','disposed','send-fail','clock-fail','set-fail','dynamic-send','stringify-fail','retained-payload']){
    const trace=[],sent=[],sentinel={units:'\uD800a\uDC00',own:undefined};let timestamp;
    const c=setup({performance:{now(){trace.push('now');if(mode==='clock-fail')throw sentinel;return -0;}},JSON:{stringify(value){trace.push('stringify');assert.equal(value.event,method==='ping'?'ping':'pong');if(mode==='stringify-fail')throw sentinel;return mode==='retained-payload'?'\uD800a\uDC00':JSON.stringify(value);}}});
    const socket=new c[channel==='public'?'WooPublicWebsocket':'WooPrivateWebscoket']({});
    const member=channel==='private'?'sendWithLog':'send';
    const send=function(value){assert.equal(this,socket);trace.push('send');sent.push(Array.from({length:value.length},(_,i)=>value.charCodeAt(i)));if(mode==='send-fail')throw sentinel;};
    let current=mode==='dynamic-send'&&method==='ping'?()=>{throw Error('stale sender')}:send;
    Object.defineProperty(socket,'isDisposed',{get(){trace.push('disposed');return mode==='disposed'?'\uD800':false;}});
    Object.defineProperty(socket,'pingAt',{set(value){trace.push('set:pingAt');assert(Object.is(value,-0));timestamp=value;if(mode==='set-fail')throw sentinel;if(mode==='dynamic-send')current=send;}});
    Object.defineProperty(socket,member,{get(){trace.push('get:'+member);return current;}});
    let caught=false;try{assert.equal(socket[method].call({}),undefined);}catch(error){assert.equal(error,sentinel);caught=true;}
    results.push({channel,method,mode,trace,sent,caught,timestampSet:timestamp!==undefined});
   }
  }
  return results;
 }
 if(kind==='heartbeat'){
  const trace=[],calls=[];const c=setup({performance:{now:()=>{trace.push('now');return 79;}},JSON:{stringify:obj=>{trace.push('stringify');return JSON.stringify(obj);}}});const r=new c.WooPublicWebsocket({});r.send=()=>{throw Error('stale send');};Object.defineProperty(r,'pingAt',{set(value){assert.equal(value,79);trace.push('set:pingAt');this.send=value=>calls.push(value);}});r.ping();r.handlePingEvent();return {trace,calls};
 }
 throw Error('unknown case '+kind);
}
run(process.argv[2]).then(result=>process.stdout.write(JSON.stringify(result))).catch(error=>{console.error(error);process.exitCode=1;});
