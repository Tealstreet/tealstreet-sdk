const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler path required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const specs=JSON.parse(fs.readFileSync(0,'utf8')),sources={},classes={},factories={};
function compile(text){const out=ts.transpileModule(text,{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert.equal(out.diagnostics.filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);return out.outputText;}
function moduleAt(relative,requireModule){const exports={};vm.runInNewContext(compile(fs.readFileSync(path.join(root,relative),'utf8')),{exports,require:requireModule});return exports;}
const logging=moduleAt('../../logging/src/LogSeverity.ts',()=>{throw Error('logging import');});
const models=moduleAt('types.ts',n=>{assert.equal(n,'@tealstreet/logging/src/LogSeverity');return logging;});
const ids=moduleAt('utils/createPositionId.ts',n=>{assert.equal(n,'../types');return models;});
const books=moduleAt('utils/orderbook.ts',()=>{throw Error('book import');});
const lodashThrottle=require(require.resolve('lodash/throttle',{paths:[path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH)]}));
for(const side of ['public','private']){
 const file=path.join(root,`exchanges/cryptocom/cryptocom.ws-${side}.ts`),text=fs.readFileSync(file,'utf8');
 const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true),klass=ast.statements.find(ts.isClassDeclaration);assert(klass);sources[side]={text,ast,klass};
 factories[side]={};for(const member of klass.members.filter(ts.isPropertyDeclaration))if(member.initializer&&(ts.isArrowFunction(member.initializer)||ts.isCallExpression(member.initializer))){const name=member.name.getText(ast);factories[side][name]=new Function('jsonParse','virtualClock','throttle','mapCandlestickInterval','sortOrderBook','calcOrderBookTotal','setTimeout','createPositionId','OrderSide','PositionSide','MarginType','"use strict";\n'+compile('return ('+member.initializer.getText(ast)+');'));}
}
function observe(v){if(v===undefined)return{$undefined:true};if(typeof v==='number'&&!Number.isFinite(v))return{$number:String(v)};if(Object.is(v,-0))return{$number:'-0'};if(typeof v==='symbol')return{$symbol:v.description};if(typeof v==='function')return{$function:[v.name,v.length]};if(Object.prototype.toString.call(v)==='[object Set]')return{$set:Array.from(v,observe)};if(Object.prototype.toString.call(v)==='[object Map]')return{$map:Array.from(v,([k,x])=>[observe(k),observe(x)])};if(Array.isArray(v))return v.map(observe);if(v&&typeof v==='object')return{$object:Reflect.ownKeys(v).filter(k=>Object.prototype.propertyIsEnumerable.call(v,k)).map(k=>[observe(k),observe(v[k])])};return v;}
(async()=>{const rows=[];for(const spec of specs){
 const side=spec.private?'private':'public',events=[],error=Error('boom'),timers=[],retained=[],loadedSymbol=Symbol('loaded-marker');let time=10000,lastBook;
 const event=(...args)=>events.push(args),fail=key=>{if(spec.fail===key)throw error;};
 const receiver={isDisposed:!!spec.disposed,isConnected:!spec.disconnected,isAuthenticated:false,messageHandlers:{},store:{marketsById:{BTC:{symbol:'BTC-SYMBOL'},ETH:{symbol:'ETH-SYMBOL'}},tickersBySymbol:{'BTC-SYMBOL':{id:'BTC'},'ETH-SYMBOL':{id:'ETH'}},positions:[{symbol:'BTC-SYMBOL',side:'long'},{symbol:'ETH-SYMBOL'},{symbol:'BTC-SYMBOL',side:'short'}],loaded:{markets:true}},parent:{}};
 receiver.parent.emitAndLogError=message=>{event('error',message);fail('emit');};receiver.parent.getPositionMode=()=>{event('mode');fail('mode');return spec.modeRejected?Promise.reject(error):Promise.resolve(spec.mode??'hedged');};
 receiver.trackLastReceived=()=>{event('track');fail('track');};
 const payload=spec.payload??{};Object.defineProperty(receiver,'recordMessageHandlerStats',{get(){if(spec.statsChange)payload.result.channel='trade';return channel=>{event('stats',channel);fail('stats');};}});
 receiver.resubscribe=()=>{event('resubscribe',receiver.isAuthenticated);fail('resubscribe');return Promise.resolve('ignored');};
 for(const name of ['handleTickerUpdate','handleTradeUpdate','handleBookUpdate','handleCandlestickUpdate','handleValuationUpdate','handleOrderUpdate','handlePositionUpdate','handleBalanceUpdate'])receiver[name]=value=>{event('delegate',name,value===payload.result);fail('handler');return Promise.resolve('ignored');};
 receiver.store.updateTicker=(ticker,patch)=>{event('ticker',ticker===receiver.store.tickersBySymbol['BTC-SYMBOL'],patch);fail('ticker');};
 receiver.store.addOrUpdateOrder=order=>{event('order',order);fail('order');};
 receiver.store.addOrUpdatePosition=position=>{event('position',position);fail('position');};
 receiver.store.update=patch=>{if(spec.loadedSymbol)assert(Reflect.ownKeys(patch.loaded).includes(loadedSymbol),'original loaded Symbol key');event('store',patch);fail('store');};
 receiver.mapOrderType=t=>t==='MARKET'?'market':t==='STOP_LIMIT'?'stop_market':'limit';
 receiver.mapOrderStatus=s=>s==='FILLED'?'closed':s==='CANCELED'?'canceled':'open';
 receiver.mapTimeInForce=t=>t==='POST_ONLY'?'PostOnly':'GoodTillCancel';
 receiver.subscribeToCandlestick=(symbol,interval)=>{event('subscribe-candle',symbol,interval);fail('subscribe');};receiver.subscribeToOrderBook=(symbol,depth)=>{event('subscribe-book',symbol,depth);fail('subscribe');};
 receiver.unsubscribe=channels=>{event('unsubscribe',channels);fail('unsubscribe');};receiver.cleanupMessageHandlerStats=handler=>{event('cleanup',handler);fail('cleanup');};
 const oldStringify=JSON.stringify,oldTimeout=global.setTimeout,oldClear=global.clearTimeout,oldNow=Date.now;
 JSON.stringify=value=>{event('frame',value);return'encoded';};receiver.send=value=>{assert.equal(value,'encoded');event('send');fail('send');};
 global.setTimeout=(fn,delay)=>{timers.push(fn);return timers.length;};global.clearTimeout=()=>{};Date.now=()=>time;
 const throttle=(fn,wait,options)=>{event('throttle',wait,options.leading,options.trailing);return lodashThrottle(fn,wait,options);};
 const args=[v=>{fail('json');return v;},{getCurrentTimeMs(){event('clock');return 777;}},throttle,i=>i==='1m'?'1m':i,books.sortOrderBook,books.calcOrderBookTotal,(fn,delay)=>{event('timer',delay);timers.push(fn);return timers.length;},ids.createPositionId,models.OrderSide,models.PositionSide,models.MarginType];
 const init=name=>factories[side][name].call(receiver,...args);
 const update=spec.update??{},callback=value=>{const same=lastBook===undefined||lastBook===value;lastBook=value;event('callback',same,structuredClone(value));fail('callback');return'ignored';};
 let result='void',out;
 try{
  switch(spec.op){
   case'route':out=init('onMessage')({data:payload});break;
   case'ticker':{const fn=init('handleTickerUpdate');out=fn(update);if(spec.second){fn(spec.second);time+=1000;timers.shift()();}break;}
   case'book':{receiver.messageHandlers['book.BTC']=[frame=>{event('fanout',frame===update);fail('fanout');return 'retained';}];out=init('handleBookUpdate')(update);break;}
   case'trade':out=init('handleTradeUpdate')(new Proxy(update,{get(){throw Error('noop read');}}));break;
   case'order':out=init('handleOrderUpdate')(update);break;
   case'position':{const promise=init('handlePositionUpdate')(update);event('before-await',events.length);if(spec.mutateAfterStart)update.data=spec.mutateAfterStart;out=await promise;break;}
   case'balance':if(spec.loadedSymbol)receiver.store.loaded[loadedSymbol]='retained';out=init('handleBalanceUpdate')(update);break;
   case'candle':case'listenbook':{
    const opts={symbol:'BTC',interval:'1m'},symbol=spec.coerce?{toString(){event('coerce');return events.filter(e=>e[0]==='coerce').length===1?'BTC':'ETH';}}:'BTC';
    const remove=spec.op==='candle'?init('listenOHLCV')(opts,callback):init('listenOrderBook')(symbol,callback);
    if(spec.removeBeforeConnect)remove();if(spec.changeSymbol)opts.symbol='ETH';if(spec.disconnected){receiver.isConnected=true;timers.shift()();}
    const key=spec.op==='candle'?'candlestick.1m.BTC':spec.coerce?'book.ETH':'book.BTC';
    for(const frame of [update,...(spec.second?[spec.second]:[])])for(const parser of receiver.messageHandlers[key]??[])parser(frame);
    if(!spec.removeBeforeConnect)remove();out=undefined;event('handlers',Object.keys(receiver.messageHandlers));break;
   }
   case'class':{
    const record=sources[side],parent={tag:'parent'},base='base';let active,target;
    class BaseWebSocket{constructor(p){assert.strictEqual(p,parent);event('super',true);fail('super');this.parent=p;}}
    const exported={};vm.runInNewContext(compile(record.text),{exports:exported,require:n=>{if(n==='../base.ws')return{BaseWebSocket};if(n==='../../types')return models;if(n==='lodash/throttle')return throttle;if(n==='../../utils/orderbook')return books;if(n.includes('jsonParse'))return{jsonParse:args[0]};if(n==='../../utils/createPositionId')return ids;if(n.includes('virtualClock'))return{virtualClock:args[1]};return{};},setTimeout:args[6],clearTimeout(){}});
    const Constructor=exported[record.klass.name.text];active=Constructor;target=spec.subclass?class Child extends Constructor{}:Constructor;
    const socket=Reflect.construct(active,[parent,'ignored'],target);assert.strictEqual(Object.getPrototypeOf(Constructor),BaseWebSocket);event('class',Constructor.name,Constructor.length);
    const fields=Object.keys(socket).filter(k=>k!=='parent');event('fields',fields.map(k=>[k,socket[k]]));retained.push(socket);break;
   }
   default:throw Error('unknown op '+spec.op);
  }
  assert.equal(out,undefined,'source void return');
 }catch(e){assert.strictEqual(e,error,'original error identity');result='original';}
 finally{JSON.stringify=oldStringify;global.setTimeout=oldTimeout;global.clearTimeout=oldClear;Date.now=oldNow;}
 rows.push({name:spec.name,result,events:observe(events),authenticated:receiver.isAuthenticated});
 }
 process.stdout.write(JSON.stringify({rows}));
})().catch(e=>{process.stderr.write(e.stack);process.exit(1);});
