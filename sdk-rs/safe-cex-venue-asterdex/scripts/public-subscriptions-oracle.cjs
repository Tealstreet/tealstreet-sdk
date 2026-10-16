// Eight unedited lifecycle methods plus their production subscribe/unsubscribe
// and already-ported decoded handlers. Full socket assembly is not substituted.
const fs=require('node:fs'),path=require('node:path'),{ts,root}=require('./source.cjs');
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts'),'utf8');
const ast=ts.createSourceFile('public-ws.ts',source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
const names=['topics','klineCallbacks','orderBookSubscriptions','subscriptionId','orderBookListenerId','pendingSubscriptions','wsReady','onOpen','listenOrderBook','subscribeOrderBook','unsubscribeOrderBookListener','clearOrderBookSubscriptionRetries','subscribeToKline','unsubscribeFromKline','dispose','subscribe','unsubscribe','handleDepthSnapshot','handleKlineUpdate'];
const members=names.map(name=>{const node=cls.members.find(n=>n.name?.getText(ast)===name);if(!node)throw Error('Missing '+name);return printer.printNode(ts.EmitHint.Unspecified,node,ast);}).join('\n');
const totals={};new Function('exports',ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/orderbook.ts'),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(totals);
const cases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/public-subscriptions-cases.json'),'utf8'));
const realSet=setTimeout,realClear=clearTimeout;let mono=0,nextId=0,timers=new Map();
// Model Node's positive finite timeout truncation, including 337.5ms backoff.
global.setTimeout=(callback,delay)=>{const id=++nextId;timers.set(id,{callback,deadline:mono+Math.max(1,Math.trunc(delay))});return id;};global.clearTimeout=id=>timers.delete(id);
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
(async()=>{const out=[];try{for(const c of cases){mono=0;timers=new Map();
 let connected=!!c.connected,fail=0,disposed=false,disposes=0,pings=0,emits=0,bookCaller=false;const attempts=[],wire=[],cleanups=[],callbacks=[],errors=[],events=[];
 const exported={};const base=class{dispose(){disposes++;connected=false;}};
 const code='export class AsterDEXPublicWebsocket extends BaseWebSocket {parent:any;ws:any;get isDisposed(){return this.parent.isDisposed;}get isConnected(){return this.ws?.readyState===1;}\n'+members+'}';
 new Function('exports','BaseWebSocket','ENABLE_RECONNECTING_WEBSOCKET','WebSocket','calcOrderBookTotal',ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exported,base,!!c.reconnecting,{OPEN:1},totals.calcOrderBookTotal);
 const socket=new exported.AsterDEXPublicWebsocket();socket.parent={get isDisposed(){return disposed;},store:{tickersBySymbol:{}}};socket.ws={get readyState(){return connected?1:0;}};
 socket.send=message=>{attempts.push(JSON.parse(message));wire.push(message);events.push('send');if(fail>0){fail--;throw Error('send-down');}};socket.ping=()=>{pings++;events.push('ping');};socket.emitWsReconnected=()=>{emits++;events.push('emit');};socket.cleanupMessageHandlerStats=stream=>cleanups.push(stream);
 // Observe only the source's void book-unsubscribe rejection. Kline's own
 // .catch intentionally consumes its send error; the source bodies stay intact.
 const unsub=socket.unsubscribe;socket.unsubscribe=(...args)=>{const work=unsub.apply(socket,args);if(bookCaller)work.catch(error=>errors.push(error.message));return work;};
 const remove=socket.unsubscribeOrderBookListener;socket.unsubscribeOrderBookListener=(...args)=>{bookCaller=true;try{return remove.apply(socket,args);}finally{bookCaller=false;}};
 const handles=new Map(),snapshots=[];
 const bookCallback=name=>book=>{callbacks.push({kind:'book',name,book:structuredClone(book)});if(name==='A')book.bids[0].amount=999;};
 const candleCallback=name=>candle=>callbacks.push({kind:'candle',name,candle:structuredClone(candle)});
 for(const step of c.steps){let pump=true;switch(step.op){
 case 'book':handles.set(step.handle,socket.listenOrderBook(step.symbol,bookCallback(step.callback)));break;
 case 'remove':handles.get(step.handle)();break;
 case 'clear-books':socket.clearOrderBookSubscriptionRetries();break;
 case 'kline':socket.subscribeToKline(step.stream,candleCallback(step.callback));break;
 case 'remove-kline':socket.unsubscribeFromKline(step.stream);break;
 case 'book-data':socket.handleDepthSnapshot(structuredClone(step.data));break;
 case 'kline-data':socket.handleKlineUpdate(structuredClone(step.data));break;
 case 'connected':connected=step.value;break;case 'disposed':disposed=step.value;break;case 'fail-send':fail=step.count;break;
 case 'open':socket.onOpen();break;case 'open-unflushed':socket.onOpen();pump=false;break;
 case 'dispose':socket.dispose();break;
 case 'tick':mono=step.value;for(const[id,timer]of [...timers])if(timers.has(id)&&timer.deadline<=mono){timers.delete(id);timer.callback();}break;
 case 'snapshot':snapshots.push({topics:structuredClone(Object.entries(socket.topics)),books:[...socket.orderBookSubscriptions].map(([stream,s])=>[stream,[...s.callbacks.keys()],s.retryTimer!==null]),klines:[...socket.klineCallbacks.keys()],pending:socket.pendingSubscriptions.length,ready:socket.wsReady,subscriptionId:socket.subscriptionId,listenerId:socket.orderBookListenerId,deadlines:[...timers.values()].map(t=>t.deadline).sort((a,b)=>a-b),attempts:attempts.length,callbacks:callbacks.length,pings,emits,disposes});break;
 default:throw Error('Unknown step '+step.op);
 }if(pump)await flush();}
 out.push({name:c.name,family:c.family,snapshots,attempts,wire,cleanups,callbacks,errors,events});socket.clearOrderBookSubscriptionRetries();timers.clear();
 }}finally{global.setTimeout=realSet;global.clearTimeout=realClear;}process.stdout.write(JSON.stringify(out));})().catch(error=>{process.stderr.write(error.stack);process.exitCode=1;});
