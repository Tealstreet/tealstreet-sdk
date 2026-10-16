const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH);const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const source=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/bitrue/bitrue.ws-public.ts');
const ast=ts.createSourceFile(source,fs.readFileSync(source,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitruePublicWebsocket');
const methods={};for(const name of ['ensureTickerSubscription','connectAndSubscribe','onOpen','onMarketsChanged','subscribe','mapKline','handleTickersEvent','makeRequest']){
 const member=klass.members.find(n=>n.name?.getText(ast)===name);assert(member&&ts.isArrowFunction(member.initializer));
 const compiled=ts.transpileModule('return ('+member.initializer.getText(ast)+');',{compilerOptions:{target:ts.ScriptTarget.ES2022}});
 methods[name]=new Function('BASE_WS_URL','nanoid','Object','JSON','dayjs','Promise','setTimeout','clearTimeout','Error',compiled.outputText);
}
const specs=JSON.parse(fs.readFileSync(path.join(__dirname,'public_bodies_fixtures.json'),'utf8'));const rows=[];
for(const spec of specs){
 const events=[],frames=[],sent=[],updates=[],listeners=[],emitter=[],timers=[],cleared=[],settled=[],callbackReturns=[];const raw=new WeakMap(),tags=new WeakMap(),error={},returned={},wire=[];tags.set(error,'original-error');tags.set(returned,'ignored-return');let id=0;let thisValue;const receivers=new Map();
 function event(name){events.push(name);if(spec.fail===name)throw error;}
 function wrap(value,tag){if(value===null||typeof value!=='object')return value;const proxy=new Proxy(value,{get(target,key,receiver){if(typeof key==='symbol'){if(key===Symbol.iterator){event('get:'+tag+':iterator');return function*(){event('iterate:'+tag);try{for(const x of target)yield x;}finally{event('iterator-close:'+tag);}};}return Reflect.get(target,key,receiver);}event('get:'+tag+':'+key);if(Array.isArray(target)){
  if(key==='forEach')return fn=>{event('call:'+tag+':forEach');for(let i=0;i<target.length;i++)if(i in target)callbackReturns.push(fn(target[i],i,proxy));return returned;};
  if(key==='map')return fn=>{event('call:'+tag+':map');return target.map((v,i)=>fn(v,i,proxy));};
  if(key==='push')return v=>{event('call:'+tag+':push');target.push(v);return target.length;};
 }return Reflect.get(target,key,receiver);},set(target,key,v){event('set:'+tag+':'+key);target[key]=Array.isArray(v)&&!raw.has(v)?wrap(v,''):v;return true;}});raw.set(proxy,value);tags.set(proxy,tag);receivers.set(tag,proxy);return proxy;}
 function un(v){return raw.get(v)??v;}
 function observe(v,seen=new Set()){
  if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};
  if(typeof v==='string')return{kind:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};
  if(typeof v==='number'){let b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')};}
  if(typeof v==='boolean')return{kind:'boolean',value:v};
  if(typeof v==='function')return{kind:'function',tag:tags.get(v)??'callback'};
  if(tags.has(v)&&!raw.has(v))return{kind:'identity',tag:tags.get(v)};
  if(seen.has(v))return{kind:'cycle'};seen=new Set(seen);seen.add(v);const value=un(v);
  if(Array.isArray(value))return{kind:'array',values:value.map(v=>observe(v,seen))};
  return{kind:'object',fields:Object.keys(value).filter(k=>typeof value[k]!=='function').map(k=>[k,observe(value[k],seen)])};
 }
 function method(tag,name,fn){const f=function(...args){event('call:'+tag+':'+name);assert.strictEqual(this,receivers.get(tag),'BITRUE-SOURCE-NATIVE-RECEIVER '+tag+':'+name);return fn(...args);};tags.set(f,tag+':'+name);return f;}
 function marker(tag){const v={};tags.set(v,tag);return v;}
 const original=wrap({close:Object.hasOwn(spec,'close')?spec.close:'2.5',rose:Object.hasOwn(spec,'rose')?spec.rose:'-0.125',vol:Object.hasOwn(spec,'vol')?spec.vol:'3.25',amount:Object.hasOwn(spec,'amount')?spec.amount:'7.5'},'json');
 const candles=(spec.candles??[{ds:'date',open:1.25,high:2.5,low:-0,close:1.5,vol:9}]).map((v,i)=>wrap(v,'candle'+i));
 const topicsRaw={};for(const [key,value]of spec.topics??[['market_a',1],['market_b',0]])topicsRaw[key]=value;const topics=wrap(topicsRaw,'topics');
 const handlersRaw={},requestsRaw={};const messageHandlers=wrap(handlersRaw,'handlers'),resolveRequests=wrap(requestsRaw,'requests');
 if(spec.existingRequest)requestsRaw.topic=wrap([],'existing-request');
 const symbols=new Set(spec.existing?['BTCUSDT']:[]);const tickerSymbols=wrap({has:method('symbols','has',v=>symbols.has(v)),add:method('symbols','add',v=>{symbols.add(v);return returned;})},'symbols');
 const markets=wrap((spec.markets??['BTCUSDT','ETHUSDT']).map((s,i)=>wrap({symbol:s},'market'+i)),'markets');
 const store=wrap({markets,updateTicker:method('store','updateTicker',(selector,ticker)=>{updates.push({selector,ticker});return returned;})},'store');
 const emitterValue=wrap({off:method('emitter','off',(...a)=>{emitter.push(['off',...a]);return returned;}),on:method('emitter','on',(...a)=>{emitter.push(['on',...a]);return returned;})},'emitter');
 const parent=wrap({store,emitter:emitterValue},'parent');
 const socket=wrap({addEventListener:method('socket','addEventListener',(event,handler)=>{listeners.push([event,handler]);return returned;})},'socket');
 const state={topics,messageHandlers,resolveRequests,tickerSymbols,parent,isDisposed:!!spec.disposed,isConnected:spec.connected!==false,ws:spec.noSocket?undefined:marker('old-socket')};
 for(const name of ['onOpen','onMessage','onClose','onMarketsChanged'])state[name]=marker(name);
 state.convertToWsSymbol=method('this','convertToWsSymbol',s=>'e_'+String(s).toLowerCase());
 state.handleTickersEvent=method('this','handleTickersEvent',(json,symbol)=>{updates.push({callbackJson:json,callbackSymbol:symbol});return returned;});
 state.ensureTickerSubscription=method('this','ensureTickerSubscription',symbol=>{updates.push({ensure:symbol});return returned;});
 state.createWebsocket=method('this','createWebsocket',url=>{listeners.push(['url',url]);return socket;});
 state.subscribe=method('this','subscribe',()=>returned);state.ping=method('this','ping',()=>returned);
 state.send=method('this','send',value=>{sent.push(value);return returned;});
 state.recordMessageHandlerStats=method('this','recordMessageHandlerStats',tag=>{updates.push({stats:tag});return returned;});
 const resolved=spec.resolvedObject?marker('resolved'):spec.resolved??'BTCUSDT';
 state.resolveMarketSymbol=method('this','resolveMarketSymbol',symbol=>{updates.push({resolve:symbol});return resolved;});
 thisValue=wrap(state,'this');
 const globals={
 BASE_WS_URL:wrap({public:'wss://fmarket-ws.bitrue.com/kline-api/ws'},'urls'),
 nanoid:()=>{event('call:global:nanoid');return 'id'+(++id);},
 Object:wrap({keys:method('Object','keys',v=>wrap(Object.keys(un(v)),'keys'))},'Object'),
 JSON:wrap({stringify:method('JSON','stringify',frame=>{frames.push(frame);const value=marker('wire'+wire.length);wire.push(value);return value;})},'JSON'),
 dayjs:ds=>{event('call:global:dayjs');assert.equal(ds,'date');return wrap({utcOffset:method('day','utcOffset',(offset,keep)=>{assert.equal(offset,480);assert.equal(keep,true);return wrap({valueOf:method('adjusted','valueOf',()=>spec.clock??1720000123456.5)},'adjusted');})},'day');},
 Promise:function(executor){event('construct:Promise');const value=marker('promise');try{executor(v=>{event('call:resolve');settled.push(['resolve',v]);},v=>{event('call:reject');settled.push(['reject',v]);});}catch(e){settled.push(['reject',e]);}return value;},
 setTimeout:(fn,delay)=>{event('call:global:setTimeout');const value=marker('timer'+timers.length);timers.push({fn,delay,value});return value;},
 clearTimeout:timer=>{event('call:global:clearTimeout');cleared.push(timer);return returned;},
 Error:function(message){event('construct:Error');assert.equal(message,'Request timeout');return marker('timeout-error');},
 };
 // Identifier reads themselves are unobservable bindings; property reads and call
 // resolution are observable. Instrumented dependencies preserve original returns.
 const fn=methods[spec.op].call(thisValue,...Object.values(globals));let result,thrown=null;
 try{let args=[];if(spec.op==='ensureTickerSubscription')args=['BTCUSDT'];if(spec.op==='onMarketsChanged')args=[wrap({added:wrap((spec.added??['BTCUSDT','ETHUSDT']).map((s,i)=>wrap({symbol:s},'added'+i)),'added')},'event')];if(spec.op==='mapKline')args=[wrap(candles,'candles')];if(spec.op==='handleTickersEvent')args=[original,'E-BTC-USDT'];if(spec.op==='makeRequest')args=['topic'];result=fn(...args);
  if(spec.fireTicker){const parser=un(Object.values(handlersRaw)[0])[0];callbackReturns.push(parser(original));}
  if(spec.fireResolve){const parser=un(requestsRaw.topic)[0];callbackReturns.push(parser(wrap({data:original},'response')));}
  if(spec.fireTimeout){callbackReturns.push(timers[0].fn());}
 }catch(e){assert.strictEqual(e,error);thrown=observe(e);}
 const tickerInfo=updates.filter(v=>v.ticker).every(v=>v.ticker.info===original);
 const candleInfo=spec.op!=='mapKline'||!result||un(result).every((v,i)=>v.info===candles[i]);
 const frameIdentities=sent.every((v,i)=>v===wire[i]);
 rows.push({name:spec.name,result:observe(result),thrown,events:[...events],socketOriginal:state.ws===socket,frames:frames.map(v=>observe(v)),sent:sent.map(v=>observe(v)),updates:updates.map(v=>observe(v)),listeners:listeners.map(v=>observe(v)),emitter:emitter.map(v=>observe(v)),topics:observe(topics),symbols:[...symbols],handlers:Object.fromEntries(Object.entries(handlersRaw).map(([k,v])=>[k,un(v).length])),requests:Object.fromEntries(Object.entries(requestsRaw).map(([k,v])=>[k,un(v).length])),timers:timers.map(v=>({delay:observe(v.delay),value:observe(v.value)})),cleared:cleared.map(v=>observe(v)),settled:settled.map(v=>observe(v)),callbackReturns:callbackReturns.map(v=>observe(v)),tickerInfo,candleInfo,frameIdentities});
}
process.stdout.write(JSON.stringify({rows}));
