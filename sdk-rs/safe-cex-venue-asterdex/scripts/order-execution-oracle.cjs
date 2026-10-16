// Unedited eight exchange methods, their actual helpers, Base preflight, and
// DefaultStore. Only REST, time scheduling, and observability sinks are seams.
const fs=require('node:fs'),path=require('node:path'),{ts,root,values}=require('./source.cjs'),load=require('./load-module.cjs');
const parse=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true),printer=ts.createPrinter();
const print=n=>printer.printNode(ts.EmitHint.Unspecified,n,n.getSourceFile());
const compile=code=>ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const cls=file=>parse(file).statements.find(n=>ts.isClassDeclaration(n));
const pick=(c,names)=>names.map(name=>{const n=c.members.find(n=>n.name?.getText(n.getSourceFile())===name);if(!n)throw Error('Missing member '+name);return print(n)}).join('\n');
const ec=cls('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts'),bc=cls('packages/safe-cex/src/exchanges/base.ts');
const names=['OPEN_INTEREST_POLL_INTERVAL_MS','OPEN_INTEREST_BATCH_SIZE','validateAccount','getAccount','getAccountEndpoint','getFapiPath','placeOrder','placeOrders','updateOrder','cancelOrders','assertV3Feature','assertAlgoOrderCapacity','getMaxAlgoOrdersLimit','getOpenAlgoOrdersCount','isAlgoOrder','mapOrderType','getOrderPositionSide','getTickSize','updateStoreOpenInterest','pollOpenInterest','startOpenInterestPolling','fetchOpenInterestMap','collectOpenInterestFromPayload'];
const enums=[parse('packages/safe-cex/src/types.ts'),parse('packages/logging/src/LogSeverity.ts')].flatMap(ast=>ast.statements.filter(ts.isEnumDeclaration)).map(print).join('\n');
const sm={};new Function('exports',compile(parse('packages/safe-cex/src/utils/safe-math.ts').statements.filter(n=>!ts.isImportDeclaration(n)).map(print).join('\n')))(sm);
const pe={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/ccxt_helpers/precision.ts'),'utf8')))(pe);
const compiler=path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)),'..'),dep=name=>require(path.join(path.dirname(compiler),name));
const enumsObject={};new Function('exports',compile(enums))(enumsObject);
const fee=load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts',[],{ethers:dep('ethers'),'../../types':enumsObject});
const clone={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/clone.ts'),'utf8')))(clone);
const storeModule={};new Function('exports','clone','virtualClock','uniqBy',compile(enums+'\n'+parse('packages/safe-cex/src/store/store.base.ts').statements.filter(n=>!ts.isImportDeclaration(n)).map(print).join('\n')))(storeModule,clone.clone,{getCurrentTimeMs:()=>1234},dep('lodash/uniqBy'));
let timers=[];
const dependencies={...values,...fee,adjust:sm.adjust,loop:(fn,delay)=>{timers.push({fn,at:time+delay});},readHttpFailureContext:()=>undefined,reportException:()=>{},formatErrorForSentry:()=>{throw Error('Unselected enhanced-error path');}};
const output={};new Function('exports',...Object.keys(dependencies),compile(enums+'\n'+parse('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts').statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText()==='isAsterDEXUnknownOrderError')).map(print).join('\n')+'\nexport class AsterDEXExchange {openInterestCursor=0;isDisposed=false;_exchangeName="asterdex";name="asterdex";clientOrderIdRule=undefined;\n'+pick(ec,names)+'\n'+pick(bc,['resolveCallerClientOrderId','assertCallerClientOrderIds','rejectPreflight','emitAndLogError','dispose'])+'}'))(output,...Object.values(dependencies));
let time=0;
const market=(symbol='BTCUSDT',max=100,extra={})=>({id:symbol,symbol,base:'BTC',quote:'USDT',active:true,precision:{price:0.01,amount:0.1},limits:{amount:{min:0.1,max},leverage:{min:1,max:20}},info:{filters:[{filterType:'PRICE_FILTER',tickSize:'0.05'},{filterType:'MAX_NUM_ALGO_ORDERS',limit:'2'}]},...extra});
const opts=(extra={})=>({symbol:'BTCUSDT',type:'limit',side:'buy',amount:1.23,price:10.03,...extra});
const order=(id='old',extra={})=>({id,status:'open',symbol:'BTCUSDT',type:'limit',side:'buy',price:10,amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:0,...extra});
const ticker=(symbol='BTCUSDT',oi=0)=>({id:symbol,symbol,bid:0,ask:0,last:1,mark:0,index:0,percentage:0,fundingRate:0,fundingTime:0,openInterest:oi,volume:0,quoteVolume:0});
const c=(name,method,args=[],queue=[],extra={})=>({name,method,args,queue,...extra});
const cases=[
 c('limit-rounded-builder-maker','placeOrder',[opts({timeInForce:'PostOnly'})],[{data:{orderId:11}}]),
 c('hedge-close-stop','placeOrder',[opts({type:'stop_market',side:'sell',amount:0,triggerPrice:11.04,slTriggerType:'last_price'})],[{data:{data:{orderId:'s1'}}}],{initial:{options:{isHedged:true}}}),
 c('oneway-reduce-takeprofit','placeOrder',[opts({type:'take_profit_market',side:'sell',triggerPrice:12.03,reduceOnly:true,tpTriggerType:'mark_price'})],[{data:{orderId:0}}]),
 c('bracket-partial-success','placeOrder',[opts({type:'market',stopLoss:8,takeProfit:12})],[{data:{orderId:'entry'}},{error:'stop down'},{data:{orderId:'tp'}}]),
 c('oversize-three-chunks','placeOrder',[opts({amount:5.3})],[{data:{orderId:'a'}},{data:{orderId:'b'}},{data:{orderId:'c'}}],{initial:{markets:[market('BTCUSDT',2)]}}),
 c('trailing-rejected-before-rest','placeOrder',[opts({type:'trailing_stop_market'})],[],{mutateErrorStore:true}),
 c('caller-id-preflight','placeOrder',[opts({clientOrderId:'caller'})]),
 c('algo-capacity-reject','placeOrder',[opts({type:'stop_market',triggerPrice:8})],[],{initial:{orders:[order('a',{type:'stop_market'}),order('b',{type:'take_profit_market'})]}}),
 c('stp-v1-rejected','placeOrder',[opts({stpMode:'EXPIRE_TAKER'})],[],{version:'v1'}),
 c('stp-v3-and-postonly-fee','placeOrder',[opts({stpMode:'EXPIRE_BOTH',postOnly:true})],[{data:{orderId:12}}]),
 c('missing-market','placeOrder',[opts({symbol:'MISSING'})]),
 c('missing-id','placeOrder',[opts()],[{data:{data:{}}}]),
 c('null-order-response','placeOrder',[opts()],[{data:null}]),
 c('batch-continues','placeOrders',[[opts(),opts({type:'market'})]],[{error:'first down'},{data:{orderId:'last'}}]),
 c('batch-all-fail','placeOrders',[[opts(),opts()]],[{error:'first down'},{error:'last down'}]),
 c('batch-preflight-no-write','placeOrders',[[opts(),opts({clientOrderId:'caller'})]]),
 c('batch-empty','placeOrders',[[]]),
 c('cancel-partial-unknown-success','cancelOrders',[[order('a'),order('b'),order('c')]],[{error:'down'},{error:'code: -2011 UNKNOWN ORDER'},{data:{}}]),
 c('cancel-all-fail','cancelOrders',[[order('a'),order('b')]],[{error:'first'},{error:'last'}]),
 c('cancel-empty','cancelOrders',[[]]),
 c('replace-stop-trigger','updateOrder',[{order:order('old',{type:'stop_market',reduceOnly:true,slTriggerType:'last_price',timeInForce:'ImmediateOrCancel'}),update:{price:13,amount:0}}],[{data:{}},{data:{orderId:'replacement'}}]),
 c('replace-cancel-failure-no-post','updateOrder',[{order:order(),update:{amount:3}}],[{error:'cancel down'}]),
 c('validate-v1','validateAccount',[],[{data:{accountAlias:'account'}}],{version:'v1'}),
 c('validate-key-missing','validateAccount',[],[],{credentials:{key:'',secret:'secret'},version:'v1'}),
 c('validate-secret-missing','validateAccount',[],[],{credentials:{key:'key',secret:''},version:'v1'}),
 c('validate-v3-missing-user','validateAccount',[],[],{credentials:{key:'key',secret:'secret'}}),
 c('validate-v3-bad-private-key','validateAccount',[],[],{credentials:{key:'0x'+'1'.repeat(40),secret:'bad',auth:{userAddress:'0x'+'2'.repeat(40)}}}),
 c('validate-v3-success','validateAccount',[],[{data:{}}],{credentials:{key:'0x'+'1'.repeat(40),secret:'a'.repeat(64),auth:{userAddress:'0x'+'2'.repeat(40)}}}),
 c('validate-null-account','validateAccount',[],[{data:null}],{version:'v1'}),
 c('validate-rest-fails','validateAccount',[],[{error:'account down',errorType:'axios'}],{version:'v1'}),
 c('interest-direct-unchanged-missing','updateStoreOpenInterest',[[['BTCUSDT',4],['UNKNOWN',8],['ETHUSDT',5]]],[],{initial:{tickers:[ticker('BTCUSDT',4),ticker('ETHUSDT',1)]}}),
 c('interest-bulk','pollOpenInterest',[],[{data:[{symbol:'BTCUSDT',openInterest:'7'}]}],{initial:{tickers:[ticker()]}}),
 c('interest-rotating-wrap','pollOpenInterest',[],[{data:[]},{data:[]},...Array.from({length:25},(_,i)=>({data:{symbol:'S'+((25+i)%27),openInterest:String(i+1)}}))],{cursor:25,initial:{tickers:Array.from({length:27},(_,i)=>ticker('S'+i))}}),
 c('interest-empty-finally-timer','pollOpenInterest'),
 c('interest-start-and-repeat','startOpenInterestPolling',[],[{data:[{symbol:'BTCUSDT',openInterest:'2'}]},{data:[{symbol:'BTCUSDT',openInterest:'3'}]}],{advance:15000,initial:{tickers:[ticker()]}}),
 c('interest-prototype-id-coercion','updateStoreOpenInterest',[[['toString',8]]],[],{initial:{tickers:[ticker('undefined',1)]}}),
 c('interest-live-map-mutation','updateStoreOpenInterest',[[['BTCUSDT',4],['ETHUSDT',5]]],[],{mutateMap:true,initial:{tickers:[ticker(),ticker('ETHUSDT',1),ticker('SOLUSDT',1)]}}),
 c('interest-native-number-boundary','updateStoreOpenInterest',[[['BTCUSDT','NaN'],['ETHUSDT','Infinity'],['ZERO','-0']]],[],{numbers:true,initial:{tickers:[ticker(),ticker('ETHUSDT',1),ticker('ZERO',0)]}}),
 c('interest-warning-log-throws-aborts-fallback','pollOpenInterest',[],[{error:'bulk down'}],{throwLog:true,initial:{tickers:[ticker()]}}),
 c('interest-disposed-no-work','pollOpenInterest',[],[],{disposed:true}),
];
(async()=>{const all=[],oldError=console.error,oldTrace=console.trace;console.error=()=>{};console.trace=()=>{};try{for(const x of cases){time=0;timers=[];const store=new storeModule.DefaultStore();store.update(structuredClone({markets:[market()],...(x.initial||{})}));let notifications=0;store.subscribe(()=>notifications++);
 const s=new output.AsterDEXExchange();s.store=store;s.options={key:'key',secret:'secret',...(x.credentials||{})};s.apiVersion=x.version||'v3';s.openInterestCursor=x.cursor||0;s.isDisposed=false;s.unsubscribeFromStore=()=>{};s.remoteSignerAvailabilityCleanups=new Set();s.stopWatchdog=()=>{};s.getAllSockets=()=>[];s.off=()=>{};if(x.disposed)s.dispose();
 const queue=structuredClone(x.queue),requests=[],logs=[],errors=[];let logFailed=false;s.log=(message,severity)=>{logs.push({message,severity});if(x.throwLog&&!logFailed&&message.startsWith('Bulk open-interest')){logFailed=true;throw Error('log down');}};s.emitter={emit:(event,message)=>{if(event==='error'){errors.push(message);if(x.mutateErrorStore)store.update({options:{isHedged:true}});}}};
 const request=(kind,method,url,body,config)=>{requests.push({kind,method,url,...(body===undefined?{}:{body}),...(config||{})});const q=queue.shift();if(!q)throw Error('Unexpected request');if(q.error)throw q.errorType==='axios'?new (dep('axios').AxiosError)(q.error):Error(q.error);return Promise.resolve({data:q.data});};
 const api=kind=>({get:(url,cfg)=>request(kind,'GET',url,undefined,cfg),post:(url,body)=>request(kind,'POST',url,body),delete:(url,cfg)=>request(kind,'DELETE',url,undefined,cfg)});s.primaryAPI=api('primary');s.secondaryAPI=api('secondary');s.publicAPI=api('public');
 let result,error;try{const args=structuredClone(x.args);if(x.method==='updateStoreOpenInterest'){args[0]=new Map(args[0].map(([k,v])=>[k,x.numbers?Number(v):v]));if(x.mutateMap){let mutated=false;store.subscribe(()=>{if(mutated)return;mutated=true;args[0].delete('ETHUSDT');args[0].set('ETHUSDT',9);args[0].set('SOLUSDT',7);});}}result=await s[x.method](...args);for(let i=0;i<32;i++)await Promise.resolve();if(x.advance){time=x.advance;const due=timers.filter(t=>t.at<=time);timers=timers.filter(t=>t.at>time);for(const t of due)t.fn();for(let i=0;i<32;i++)await Promise.resolve();}}catch(e){error={name:e.name,message:e.message};}
 all.push({...x,initial:{markets:[market()],...(x.initial||{})},credentials:s.options,version:s.apiVersion,result:result??null,error:error||null,requests,logs,errors,notifications,hedged:store.options.isHedged,cursor:s.openInterestCursor,timers:timers.map(t=>t.at),tickers:structuredClone(store.tickers),unconsumed:queue.length});
 }}finally{console.error=oldError;console.trace=oldTrace;}process.stdout.write(JSON.stringify(all,(_,v)=>typeof v==='number'&&!Number.isFinite(v)?{number:String(v)}:v));})().catch(e=>{process.stderr.write(e.stack);process.exitCode=1;});
