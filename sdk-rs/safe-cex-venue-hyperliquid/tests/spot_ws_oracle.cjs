const fs=require('fs'),path=require('path'),vm=require('vm'),crypto=require('crypto');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),sourceHashes={};
function source(file){const t=fs.readFileSync(path.join(root,file),'utf8');sourceHashes[file]=crypto.createHash('sha256').update(t).digest('hex');return t;}
function declarations(file,names){const t=source(file),a=ts.createSourceFile(file,t,ts.ScriptTarget.Latest,true);return a.statements.filter(s=>names.includes(s.name?.text)||(ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>names.includes(d.name.getText(a))))).map(s=>s.getText(a)).join('\n');}
function moduleOf(t,imports){const c={exports:{},require:n=>{if(!(n in imports))throw Error('unbound '+n);return imports[n]},performance:{now:()=>123.5},JSON:{parse:wire=>{const v=JSON.parse(wire);if(state?.input.channelSequence){let read=0;Object.defineProperty(v,'channel',{enumerable:true,get(){const c=state.input.channelSequence[read++];state.record('channel-read',c);return c}})}return v},stringify:v=>{state.record('serialize',v);return JSON.stringify(v)}}};vm.runInNewContext(ts.transpileModule(t,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,c);return c.exports;}
const enums=moduleOf(declarations('packages/safe-cex/src/types.ts',['OrderSide']),{});
const tables=moduleOf(declarations('packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.types.ts',['WSS_URLS']),{});
const constants=moduleOf(declarations('packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.constants.ts',['systemSubscriptionTypes']),{});
const json=moduleOf(source('packages/json-parse/src/jsonParse.ts'),{});
let state;
class BaseWebSocket {
 constructor(parent){if(state.input.baseThrow)throw 'base';this.parent=parent;this.isDisposed=!!state.input.disposed;this.baseToken=parent.token;}
 get store(){return this.parent.store}
 trackLastReceived(){state.record('track');if(state.input.trackThrow)throw undefined;}
 recordMessageHandlerStats(v){state.record('stats',v);if(state.input.statsThrow)throw 'stats';}
 handlePongEvent(){state.record('pong')}
 createWebsocket(url){state.record('create',url);return {addEventListener:(name,fn)=>state.record('listen',name,fn===this[{open:'onOpen',message:'onMessage',close:'onClose'}[name]])}}
 onClose(){}
 send(wire){JSON.parse(wire);state.record('send')}
}
const imports={'@tealstreet/json-parse/src/jsonParse':{jsonParse:d=>(state.parsed=json.jsonParse(d))},'../../types':enums,'../base.ws':{BaseWebSocket},'./hyperliquidspot.constants':constants,'./hyperliquidspot.types':tables,'lodash/flatten':{default:()=>{throw 'unused flatten'}},nanoid:{nanoid:()=>{throw 'unused nanoid'}},'../../utils/orderbook':{},'../../utils/safe-math':{}};
const Private=moduleOf(source('packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.ws-private.ts'),imports).HyperliquidSpotPrivateWebSocket;
const Public=moduleOf(source('packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.ws-public.ts'),imports).HyperliquidSpotPublicWebSocket;
function tag(v){if(v===undefined)return ['u'];if(v===null)return ['null'];if(typeof v==='boolean')return ['bool',v];if(typeof v==='number'){if(Number.isNaN(v))return ['number','nan'];const b=Buffer.alloc(8);b.writeDoubleBE(v);return ['number',b.toString('hex')];}if(typeof v==='string')return ['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))];if(Object.prototype.toString.call(v)==='[object Error]')return ['error',v.name,v.message];if(Array.isArray(v))return ['array',v.map(tag)];return ['object',Object.keys(v).map(k=>[k,tag(v[k])])];}
const cases=[];function add(family,name,input={}){cases.push({family,name,input})}
for(const[name,input]of [['ordinary',{}],['disposed',{disposed:true}],['lexical',{lexical:true}]])add('class',name,input);
for(const[name,input]of [['ordinary',{}],['custom-target',{customTarget:true}],['base-throws',{baseThrow:true}],['prototype-throws',{prototypeThrow:true}]])add('constructor',name,input);
const fill={tid:17,side:'B',coin:'BTC',px:'123.5tail',sz:'2.25x'};
for(const[name,input]of [['buy-sell',{data:{fills:[fill,{...fill,tid:null,side:'A',coin:'UNKNOWN',px:'bad',sz:'-0'}]}}],['snapshot',{data:{isSnapshot:true,fills:[fill]}}],['empty',{data:{}}],['null',{data:null}],['throw-emit',{data:{fills:[fill]},emitThrow:true}],['live-append',{data:{fills:[fill]},appendFill:true}],['opaque-symbol',{data:{fills:[fill]},opaqueSymbol:true}]])add('fills',name,input);
for(const[name,input]of [['balances',{data:{balances:[{coin:'USDC',total:'42'}]}}],['missing',{data:{}}],['null',{data:null}],['false',{data:{balances:false}}],['throws',{data:{balances:[]},balancesThrow:true}]])add('balances',name,input);
for(const[name,input]of [['ordinary',{data:{fundings:[1]}}],['undefined',{}],['poison',{poison:true}]])add('fundings',name,input);
for(const[name,input]of [['ordinary',{data:{nonFundingLedgerUpdates:[1]}}],['undefined',{}],['throws',{refreshThrow:true}]])add('ledger',name,input);
for(const frame of [{channel:'spotClearinghouseState',data:{balances:[{coin:'USDC'}]}},{channel:'spotState',data:{spotState:{balances:[{coin:'BTC'}]}}},{channel:'spotState',data:{balances:[{coin:'HYPE'}]}},{channel:'userFills',data:{fills:[fill]}},{channel:'userFundings',data:{fundings:[1]}},{channel:'userNonFundingLedgerUpdates'},{channel:'pong'},{channel:'unknown'},{channel:'activeAssetData',data:{coin:'BTC'}},{channel:'activeAssetData',data:{}},null,42])add('private',String(cases.filter(c=>c.family==='private').length),{wire:JSON.stringify(frame)});
for(const[name,input]of [['malformed',{wire:'bad'}],['disposed',{wire:'{}',disposed:true,eventGetter:true}],['null-event',{eventNull:true}],['undefined-event',{eventUndefined:true}],['track-undefined',{wire:'{}',trackThrow:true}],['stats-throws',{wire:'{"channel":"pong"}',statsThrow:true}],['event-throws',{eventThrow:true}]])add('private',name,input);
for(const frame of [{channel:'pong'},{channel:'error'},{channel:'allMids',data:{mids:{BTC:'12.75x',UNKNOWN:'bad',ETH:'-0'}}},{channel:'allMids'},{channel:'candle',data:[{s:'BTC',i:'1m',c:'3'}]},{channel:'candle',data:{s:'BTC',i:'1m'}},{channel:'candle',data:[]},{channel:'candle',data:{s:'BTC'}},{channel:'l2Book',data:{coin:'BTC',levels:[]}},{channel:'l2Book',data:{}},null,42])add('public',String(cases.filter(c=>c.family==='public').length),{wire:JSON.stringify(frame)});
for(const channel of ['candle','l2Book'])for(const flag of ['handlerThrow','appendHandler'])add('public',channel+'-'+flag,{wire:JSON.stringify({channel,data:channel==='candle'?[{s:'BTC',i:'1m'}]:{coin:'BTC'}}),[flag]:true});
for(const[name,input]of [['null-event',{eventNull:true}],['disposed',{wire:'{}',disposed:true,eventGetter:true}],['track-throws',{wire:'{}',trackThrow:true}],['malformed',{wire:'bad'}]])add('public',name,input);
for(const[name,input]of [['known',{data:{BTC:'17.25tail',UNKNOWN:'bad',ETH:'-0'}}],['inherited',{data:{BTC:'1'},inherited:true}],['nan',{data:{BTC:'bad'}}],['mutate',{data:{BTC:'3',ETH:'4'},mutateTicker:true}],['null',{data:null}],['unknown-poison',{data:{UNKNOWN:'x'},unknownPoison:true}]])add('mids',name,input);
add('class','connect-open',{chain:true});
add('public','changing-channel',{wire:'{}',channelSequence:['other','candle','pong']});
add('private','changing-channel',{wire:'{}',channelSequence:['other','pong']});
function execute(c){const i=c.input,input=tag(i),effects=[];state={input:i,parsed:undefined,record:(...v)=>effects.push(tag(v))};const token={},symbol=i.opaqueSymbol?{symbol:true}:'BTC-USDC';const positions=[{symbol:'BTC-USDC',contracts:2,marginType:'isolated',leverage:7},{symbol:'OTHER',contracts:0},{symbol:'BTC-USDC',contracts:0,marginType:'isolated',leverage:1}];const tickers={BTC:{id:'BTC'},ETH:{id:'ETH'}};let socket,buffer;const parent={token,options:{testnet:false,vault:'vault',applicationId:'app'},activeAssetData:new Map(),store:{markets:[{id:'BTC'}],marketsById:{BTC:{symbol}},positions,tickersById:tickers,updateTicker(t,b){state.record('ticker',t,b,t===tickers[t.id],b===buffer,this===parent.store);if(i.mutateTicker){b.marker=true;parent.store={...parent.store,updateTicker:function(t,b){state.record('ticker-new',t,b,t===tickers[t.id],b===buffer,this===parent.store)}};}},updatePosition(p,changes){state.record('position',p,changes,positions.includes(p))}},_getSymbolConfigurationFromAssetData(coin){state.record('config',coin);return {positionMode:'oneway',marginType:'cross',leverage:3}},applySpotBalances(b){state.record('balances',b,b===(i.data?.balances??state.parsed?.data?.spotState?.balances??state.parsed?.data?.balances));if(i.balancesThrow)throw 'balances'},requestSpotBalanceRefresh(...a){state.record('refresh',...a);if(i.refreshThrow)throw 'refresh'},emitter:{emit(event,r){state.record('emit',event,r,r.symbol===symbol);if(i.emitThrow)throw 'emit';if(i.appendFill&&i.data.fills.length===1)i.data.fills.push({...fill,tid:18});}}};
const originalSet=parent.activeAssetData.set;parent.activeAssetData.set=function(k,v){state.record('asset',k,v,v===state.parsed?.data);return originalSet.call(this,k,v)};
let result,error,rejected=false;try{
 if(c.family==='constructor'){
  let Target=Private,custom={};if(i.prototypeThrow){Target=new Proxy(function Custom(){},{get(t,k,r){if(k==='prototype')throw 'prototype';return Reflect.get(t,k,r)}});}
  if(i.customTarget){Target=function Custom(){};custom=Target.prototype;}
  socket=Reflect.construct(Private,[parent],Target);
  result={parent:socket.parent===parent,token:socket.baseToken===token,prototype:Object.getPrototypeOf(socket)===(i.customTarget?custom:Private.prototype),keys:Object.keys(socket),arrows:Object.keys(socket).slice(3).map(k=>[k,typeof socket[k],socket[k].name,socket[k].length,Object.getOwnPropertyDescriptor(socket,k).enumerable])};
 }else{
  socket=new(c.family==='public'||c.family==='mids'?Public:Private)(parent);buffer=socket._midsUpdateBuf;
  if(c.family==='class'){
   const other=new Private(parent);const names=Object.keys(socket).slice(3);if(i.lexical){const fn=socket.handleUserNonFundingLedgerUpdates;fn.call({parent:{requestSpotBalanceRefresh(){throw 'wrong receiver'}}});}
   if(i.chain){socket.connectAndSubscribe();socket.onOpen();}
   result={name:Private.name,length:Private.length,base:Object.getPrototypeOf(Private)===BaseWebSocket,prototypeConstructor:Private.prototype.constructor===Private,prototypeKeys:Object.getOwnPropertyNames(Private.prototype),constructorEnumerable:Object.getOwnPropertyDescriptor(Private.prototype,'constructor').enumerable,shared:Object.getPrototypeOf(socket)===Object.getPrototypeOf(other),distinct:names.every(k=>socket[k]!==other[k]),arrows:names.map(k=>[k,typeof socket[k],socket[k].name,socket[k].length])};
  }else if(c.family==='mids'){if(i.inherited)Object.setPrototypeOf(i.data,{ETH:'9.5'});if(i.unknownPoison)Object.defineProperty(i.data,'UNKNOWN',{enumerable:true,get(){throw 'unknown read'}});result=socket.handleAllMids(i.data)}
  else if(c.family==='public'||c.family==='private'){
   if(c.family==='public'){for(const key of ['candle.BTC.1m','l2Book.BTC']){const handlers=[];function handler(n){return function(payload){state.record('handler',n,this===handlers,payload===state.parsed.data,payload);if(n===0&&i.appendHandler)handlers.push(handler(2));if(n===0&&i.handlerThrow)throw undefined;}}handlers.push(handler(0),handler(1));socket.messageHandlers[key]=handlers;}}
   let event={data:i.wire};if(i.eventGetter)Object.defineProperty(event,'data',{get(){state.record('event-data');return i.wire}});if(i.eventThrow)Object.defineProperty(event,'data',{get(){throw 'event'}});if(i.eventNull)event=null;if(i.eventUndefined)event=undefined;result=socket.onMessage(event);
  }else{const method={balances:'handleSpotClearinghouseState',fills:'handleFills',fundings:'handleFundings',ledger:'handleUserNonFundingLedgerUpdates'}[c.family];const data=i.poison?new Proxy({},{get(){throw 'poison'}}):i.data;result=socket[method](data)}
 }
}catch(e){rejected=true;error=e;}
return {family:c.family,name:c.name,input,result:tag(result),error:tag(error),rejected,effects:['array',effects]};}
process.stdout.write(JSON.stringify({sourceHashes,tables:tag(tables.WSS_URLS),systems:tag(constants.systemSubscriptionTypes),cases:cases.map(execute)}));
