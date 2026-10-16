const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.ws-public.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BlofinPublicWebsocket');
const declarations=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).filter(d=>['BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES','buildBlofinSubscribeFrames'].includes(d.name.text));
const classText=declarations.map(d=>'const '+d.getText(ast)+';').join('\n')+'\n'+klass.getText(ast).replace(/^export /,'')+'\nreturn BlofinPublicWebsocket;';
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {BASE_WS_URL,INTERVAL}=load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
const {jsonParse}=load(path.join(root,'packages/json-parse/src/jsonParse.ts'));
const {getCandleStartTime}=load(path.join(root,'packages/safe-cex/src/utils/getCandleStartTime.ts'));
const {calcOrderBookTotal}=load(path.join(root,'packages/safe-cex/src/utils/orderbook.ts'));
const {updateOrderBook}=load(path.join(root,'packages/safe-cex/src/utils/updateOrderBook.ts'));
const {roundUSD}=load(path.join(root,'packages/safe-cex/src/utils/roundUSD.ts'));
const market={id:'BTC-USDT',symbol:'BTC/USDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.001,price:0.1},limits:{amount:{min:0.001,max:100},leverage:{min:1,max:100}},contractSize:1,isPerpetual:true};
const ticker={id:market.id,symbol:market.symbol,bid:99,ask:101,last:100,mark:100,index:100,percentage:0,openInterest:4,fundingRate:0,fundingTime:0,volume:5,quoteVolume:6};
const opts={symbol:market.symbol,interval:'1m'};
const tick={instId:market.id,open24h:'80',last:'100',bidPrice:'99',askPrice:'101',volCurrency24h:'23',vol24h:'2300'};
const candle=[1710000000000,'100','110','90','105','7',null,'8'];
const message=(channel,data,rest={})=>JSON.stringify({arg:{channel,instId:market.id},data,...rest});
const book=(bids=[['99','2'],['98','3']],asks=[['101','4'],['102','5']],seqId='10',prevSeqId)=>({bids,asks,seqId,...(prevSeqId===undefined?{}:{prevSeqId})});
const snap=()=>message('books',[book()],{action:'snapshot'});
const cases=[],add=(group,actions,initial={})=>cases.push({group,actions,initial});
for(const i of [{},{fail:'base-close'},{disposed:true}])add('constructor',[['close',{code:1006}],['close',{code:1000}]],i);
for(const i of [{},{timeout:5},{timeout:0},{timeout:5,fail:'clear'},{fail:'off'},{fail:'super-dispose'}])add('dispose',[['dispose']],i);
for(const [raw,i] of [['pong',{}],['pong',{disposed:true}],['plain',{}],['{"channel":""}',{}],['{"channel":"books"',{}],['{"channel":"books","event":"subscribe"}',{}],['{"channel": "books","data":[]}',{}],['{"channel":"books","data":[]}',{}],['{"channel":"books","data":[],"instId":"BTC-USDT"}',{}],['{"channel":"books","data":[],"instId":""}',{}],['{"channel":"books","data":[],"instId":"BTC-USDT"}',{fail:'custom:first'}],['{"channel":"books","data":[]}',{timeout:5,fail:'clear'}],['{"channel":"books","data":[]}',{fail:'track'}],['{"channel":"constructor","data":[]}',{boundary:'prototype'}],['{"channel":"books","data":["\\ud800"]}',{boundary:'surrogate'}],['{"channel":"books","data":'+ '['.repeat(130)+'42'+']'.repeat(130)+'}',{boundary:'depth'}],['{"channel":"books","data":[],"nested":{"instId":"BTC-USDT"}}',{}],['{"channel":"books","data":[]} trailing',{}]])add('message',[['custom'],['message',raw]],i);
for(const update of [tick,{...tick,open24h:'0'}, {...tick,last:'-0',bidPrice:'bad',askPrice:'Infinity'}, {...tick,instId:'missing'}, {...tick,last:[12],open24h:'8tail'}, {...tick,last:{toString:0}}, {...tick,last:null}])add('ticker',[['ticker',{data:[update,tick]}]]);
for(const update of [{instId:market.id,fundingRate:'0.0001',fundingTime:'1710000123456'}, {instId:market.id,fundingRate:'-0',fundingTime:'bad'}, {instId:'missing',fundingRate:'2',fundingTime:'3'}, {instId:market.id,fundingRate:['1.2'],fundingTime:'4tail'}])add('funding',[['funding',{data:[update]}]]);
add('ticker-shape',[['ticker',{data:[]}]]);add('ticker-shape',[['ticker',{data:null}]]);
for(const [actions,i] of [
 [[['candle',opts],['message',message('candle1m',[candle])],['message',message('candle1m',[candle])]],{}],
 [[['candle',opts],['message',message('candle1m',[candle])],['message',message('books5',[{ts:'1710000000000',bids:[['106','1']],asks:[['108','1']]}])]],{}],
 [[['candle',opts],['message',message('books5',[{ts:0,bids:[],asks:[]}])]],{}],
 [[['candle',opts],['message',message('candle1m',{ts:'1710000000000tail',o:'1',h:'3',l:'0',c:'2',v:'4'})]],{}],
 [[['candle',opts],['message',message('candle1m',{timestamp:'1000',open:'1',high:'3',low:'0',close:'2'})]],{}],
 [[['candle',opts],['message',message('candle1m',[[1710000000000,'1','2','0','NaN']])]],{}],
 [[['candle',opts],['message',message('candle1m',[[0,'1','2','0','1',null,'9']])]],{}],
 [[['candle',opts],['message',message('candle1m',[candle])],['message',message('books5',{ts:'1710000060000',bids:[['106','1']],asks:[['108','1']]})]],{}],
 [[['candle',opts],['message',message('candle1m',[candle])],['message',message('candle1m',[candle])]],{mutateCandle:true}],
 [[['candle',{...opts,symbol:'missing'}],['cleanup',0]],{}],
 [[['candle',opts],['candle',opts],['cleanup',0],['message',message('candle1m',[candle])],['cleanup',1]],{}],
 [[['candle',opts],['cleanup',0],['cleanup',0]],{}],
 [[['candle',opts],['message',message('candle1m',[candle])],['message',message('books5',[{ts:'1710000000000',bids:[['NaN','1']],asks:[['108','1']]}])]],{}],
 [[['candle',opts],['message',message('candle1m',[candle])]],{fail:'candle-callback'}],
 [[['candle',opts]],{fail:'send:2'}],
])add('candle',actions,i);
for(const [actions,i] of [
 [[['book',market.symbol],['message',snap()],['message',message('books',[book([['99','0'],['97','7']],[['101','2']],11,10)],{action:'update'})]],{}],
 [[['book',market.symbol],['message',message('books',[book()],{action:'update'})],['message',message('books',[book()],{action:'update'})],['message',snap()]],{}],
 [[['book',market.symbol],['message',snap()],['message',message('books',[book([],[],12,999)],{action:'update'})]],{}],
 [[['book',market.symbol],['message',snap()],['message',message('books5',book([['96','0'],['97','9']],[['103','8']]))]],{}],
 [[['book',market.symbol],['message',message('books',[book([['101','1'],['99','2']],[['99','1'],['102','2']])],{action:'snapshot'})]],{}],
 [[['book',market.symbol],['message',message('books',{bids:[],asks:[],seqId:0},{action:'snapshot'})],['message',message('books',{bids:[['99','3']],asks:[],seqId:1,prevSeqId:0},{action:'update'})]],{}],
 [[['book',market.symbol],['message',snap()],['message',message('books',{data:null},{action:'snapshot'})]],{}],
 [[['book',market.symbol],['message',snap()],['message',message('books',[book()],{action:'other'})]],{}],
 [[['book',market.symbol],['message',snap()],['message',message('books5',[book([['99','1']],[['101','1']])])],['cleanup',0]],{}],
 [[['book',market.symbol],['message',snap()],['message',message('books5',[book([['99','1']],[['101','1']])])]],{mutateBook:true}],
 [[['book','missing'],['cleanup',0]],{}],
 [[['book',market.symbol],['book',market.symbol],['message',snap()],['cleanup',0],['cleanup',1]],{}],
 [[['book',market.symbol],['message',snap()]],{fail:'book-callback'}],
 [[['book',market.symbol],['message',message('books',[book([],[],1,0)],{action:'update'})]],{fail:'send:3'}],
])add('book',actions,i);
for(const [actions,i] of [
 [[['candle',opts],['cleanup',0]],{loaded:false}], [[['book',market.symbol],['cleanup',0]],{loaded:false}],
 [[['candle',opts],['loaded',true],['fire',1],['cleanup',0]],{loaded:false}],
 [[['book',market.symbol],['loaded',true],['fire',1],['cleanup',0]],{loaded:false}],
 [[['candle',opts],['connected',true],['fire',1],['cleanup',0]],{connected:false}],
 [[['book',market.symbol],['disposed',true],['connected',true],['fire',1],['cleanup',0]],{connected:false}],
 [[['book',market.symbol],['fire',1],['fire',2],['cleanup',0]],{connected:false}],
 [[['candle',opts],['cleanup',0]],{connected:false,firstTimer:0}],
])add('retry',actions,i);
add('facade',[['connect'],['open'],['message',message('tickers',[tick])],['dispose']]);
add('dispatch-live',[['custom'],['message','{"channel":"books","data":[]}']],{appendDuringDispatch:true,mutatePayload:true});
add('candle-reentry',[['candle',opts],['message',message('candle1m',[candle])]],{reenterCandle:true});
add('book-reentry',[['book',market.symbol],['message',snap()]],{cleanupOnBook:true});
add('book-reentry',[['book',market.symbol],['message',message('books',[book([],[],1,0)],{action:'update'})]],{reenterSend:true});
add('book-boundary',[['book',market.symbol],['message',message('books',[book([[{toString:0},'0'],'34','🚀0','🚀2'],[])],{action:'snapshot'})]],{last:0});
add('book-boundary',[['book',market.symbol],['message',snap()],['message',message('books5',[book([['97','9'],[{toString:0},'1']],[])])]],{});
add('book-boundary',[['book',market.symbol],['message',snap()],['message',message('books5',[book([[{toString:0},'0'],'34','🚀0','🚀2'],[])])]],{});
add('options-alias',[['candle',opts],['opts-symbol','missing'],['loaded',true],['fire',1],['cleanup',0]],{loaded:false});
add('candle-string-book',[['candle',opts],['message',message('candle1m',[[1710000000000,'1','5','0','2','1']])],['message',message('books5',[{ts:'1710000000000',bids:['34'],asks:['45']}])]],{});
add('candle-coercion-order',[['candle',opts],['message',message('candle1m',{ts:'bad',open:{toString:0},high:'3',low:'0',close:'2'})]],{});
add('book-shape',[['book',market.symbol],['message',message('books',[book([1],[])],{action:'snapshot'})]],{});
add('book-shape',[['book',market.symbol],['message',snap()],['message',message('books',[book([null],[],11,10)],{action:'update'})]],{});
add('book-shape',[['book',market.symbol],['message',snap()],['message',message('books5',[book([{}],[])])]],{});
add('default-reentry',[['custom-default'],['message',message('tickers',[tick])]],{storeReentry:true});
add('default-reentry',[['custom-default','funding-rate'],['message',message('funding-rate',[{instId:market.id,fundingRate:'0.0001',fundingTime:'1710000123456'}])]],{storeReentry:true});
const floors={constructor:3,dispose:6,message:18,ticker:7,funding:4,'ticker-shape':2,candle:15,book:14,retry:8,facade:1,'dispatch-live':1,'candle-reentry':1,'book-reentry':2,'book-boundary':3,'options-alias':1,'candle-string-book':1,'candle-coercion-order':1,'book-shape':3,'default-reentry':2};
assert.equal(cases.length,93,'independent93casefloor');for(const [group,floor]of Object.entries(floors))assert.equal(cases.filter(c=>c.group===group).length,floor,group+'floor');
function observe(v){if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:b.toString('hex')}}if(Array.isArray(v))return v.map(observe);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,observe(v)]));return v;}
function run(original){
 const input=structuredClone(original),c=structuredClone(original);
 const i=c.initial,store=new DefaultStore(),trace=[],sent=[],calls=[],timers=new Map(),cleanups=[],retained=[],retainedArrays=[];let reentered=false,sharedPayload=null;let next=i.firstTimer??1,sends=0;
 store.update({markets:[structuredClone(market)],tickers:[{...structuredClone(ticker),last:i.last??ticker.last}],loaded:{markets:i.loaded!==false,tickers:true,balance:false,positions:false,orders:false}});
 const call=label=>{if(i.fail===label)throw Error(label+' failed')};
 class BaseWebSocket{
  constructor(parent){call('super-construct');trace.push(['super-construct']);this.parent=parent;this.store=store;this.isDisposed=!!i.disposed;this.isConnected=i.connected!==false;this.pingAt=0;this.ws=null;this.onClose=event=>{call('base-close');trace.push(['base-close',event??null])};}
  send(frame){sends++;call('send:'+sends);sent.push(frame==='ping'?{ping:true}:JSON.parse(frame));trace.push(['send',frame]);if(i.reenterSend&&!reentered&&JSON.parse(frame).op==='unsubscribe'){reentered=true;stream.onMessage({data:snap()})}}
  createWebsocket(url){trace.push(['create',url]);return{addEventListener(event){trace.push(['listener',event])}}}
  dispose(){call('super-dispose');trace.push(['super-dispose']);this.isDisposed=true;}
  trackLastReceived(){call('track');trace.push(['track'])}
  handlePongEvent(){call('pong');trace.push(['pong'])}
  cleanupMessageHandlerStats(key){call('cleanup');trace.push(['cleanup',key])}
  emitWsReconnected(){trace.push(['reconnected'])}
 }
 const ctx={BaseWebSocket,BASE_WS_URL,INTERVAL,ENABLE_RECONNECTING_WEBSOCKET:true,LogSeverity:{Warning:'warning'},jsonParse,flatten:dependencies('lodash/flatten'),getCandleStartTime,calcOrderBookTotal,updateOrderBook,roundUSD,performance:{now:()=>123.5},setTimeout(cb,delay){call('timer');const id=next++;timers.set(id,cb);trace.push(['timer',id,delay]);return id},clearTimeout(id){call('clear');timers.delete(id);trace.push(['clear',id])}};
 const factory=new Function(...Object.keys(ctx),ts.transpileModule('return function(){'+classText+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(ctx));
 const parent={options:{testnet:false},emitter:{off(){call('off');trace.push(['off'])},on(){trace.push(['on'])},emit(e,m,o){trace.push(['warning',m,o.forceToast])}},log(m,s){call('log');trace.push(['log',m,s])}};
 const stream=new(factory())(parent);stream.connectionFailureNoticeTimeout=i.timeout;
 if(i.storeReentry)Object.defineProperty(stream,'store',{get(){if(sharedPayload){sharedPayload.nativeMutated=77;sharedPayload.data[0].bidPrice='88';sharedPayload.data[0].fundingRate='0.0002';}return store;}});
 const snapshots=[],results=[];
 for(const[action,arg]of c.actions){try{
  if(action==='custom-default'){const key=arg??'tickers',original=stream.messageHandlers[key][0];stream.messageHandlers[key]=[data=>{sharedPayload=data;calls.push({kind:'capture',payload:observe(data)});trace.push(['custom','capture'])},original,data=>{calls.push({kind:'after-store',payload:observe(data)});trace.push(['custom','after-store'])}];}else if(action==='custom'){for(const[key,names]of [['books',['first','second']],['books.BTC-USDT',['exact']]])stream.messageHandlers[key]=names.map(name=>data=>{call('custom:'+name);if(i.boundary==='surrogate'){assert.equal(data.data[0].charCodeAt(0),0xd800,'source native lone-surrogate');calls.push({kind:name,units:[0xd800]});}else if(i.boundary==='depth'){let leaf=data.data,depth=0;while(Array.isArray(leaf)){depth++;leaf=leaf[0]}assert.equal(depth,130);assert.equal(leaf,42);calls.push({kind:name,depth,leaf});}else {if(i.mutatePayload&&name==='first')data.nativeMutated=77;calls.push({kind:name,payload:observe(data)});if(i.appendDuringDispatch&&name==='first')stream.messageHandlers.books.push(d=>{calls.push({kind:'appended',payload:observe(d)});trace.push(['custom','appended'])});}trace.push(['custom',name])})}
  else if(action==='message')stream.onMessage({data:arg});else if(action==='ticker')stream.handleTickerEvents(arg);else if(action==='funding')stream.handleFundingRateEvents(arg);
  else if(action==='candle')cleanups.push(stream.listenOHLCV(arg,candle=>{call('candle-callback');retained.push(candle);calls.push({kind:'candle',payload:observe(candle)});if(i.mutateCandle)candle.close+=1;if(i.reenterCandle&&!reentered){reentered=true;stream.onMessage({data:message('candle1m',[[1710000000000,'100','110','90','106','7',null,'8']])})}}));
  else if(action==='book')cleanups.push(stream.listenOrderBook(arg,book=>{call('book-callback');retained.push(book);retainedArrays.push(book.bids);calls.push({kind:'book',payload:observe(book)});if(i.mutateBook&&book.bids[0])book.bids[0].amount=77;if(i.cleanupOnBook)cleanups[0]();}));
  else if(action==='cleanup')cleanups[arg]?.();else if(action==='close')stream.onClose(arg);else if(action==='fire'){const cb=timers.get(arg);timers.delete(arg);cb?.()}
  else if(action==='opts-symbol')c.actions[0][1].symbol=arg;else if(action==='loaded')store.update({loaded:{...store.loaded,markets:arg}});else if(action==='connected')stream.isConnected=arg;else if(action==='disposed')stream.isDisposed=arg;
  else stream[{connect:'connectAndSubscribe',open:'onOpen'}[action]??action]();results.push(null);
 }catch(e){results.push({name:e.name,message:e.message})}
 snapshots.push({handlers:Object.entries(stream.messageHandlers).map(([k,v])=>[k,v.length]),topics:Object.entries(stream.topics),timeout:stream.connectionFailureNoticeTimeout??null,notified:stream.connectionFailureNotified,disposed:stream.isDisposed,connected:stream.isConnected,ticker:observe(store.tickers[0]),loaded:store.loaded.markets});
 }
 if(c.group==='book'&&retained.length>1&&c.actions.filter(a=>a[0]==='book').length===1){assert(retained.every(ref=>ref===retained[0]),'source repeated book callback alias');}
 let defaultCallbackLast=null;if(c.group==='facade'){const retainedDefault=stream.messageHandlers.tickers[0];retainedDefault({data:[{...tick,last:'999'}]});defaultCallbackLast=store.tickers[0].last;assert.equal(defaultCallbackLast,999,'source retained default callback remains callable');}return{...input,results,snapshots,calls,sent,trace,retained:retained.map(observe),retainedArrays:retainedArrays.map(observe),defaultCallbackLast};
}
const output=cases.map(run);
assert.equal(output.find(c=>c.group==='ticker').snapshots[0].ticker.volume.number,'4037000000000000','source native base volume23');
assert.equal(output.find(c=>c.group==='candle').calls.length,1,'source duplicate candle suppressed');
assert.equal(output.find(c=>c.group==='retry'&&c.actions.some(a=>a[0]==='loaded')&&c.actions[0][0]==='candle').snapshots.at(-1).handlers.find(([k])=>k==='candle1m.BTC-USDT')[1],1,'original cleanup does not own retried listener');
process.stdout.write(JSON.stringify({market,ticker,cases:output,floors,stoppedAfter:93}));
