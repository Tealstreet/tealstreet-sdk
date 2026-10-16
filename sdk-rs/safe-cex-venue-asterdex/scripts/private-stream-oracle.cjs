// Full unedited private class, real DefaultStore, injected socket/time/HTTP seams.
const fs=require('node:fs'),path=require('node:path'),{ts,root,values}=require('./source.cjs'),load=require('./load-module.cjs');
const parse=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true),printer=ts.createPrinter(),print=n=>printer.printNode(ts.EmitHint.Unspecified,n,n.getSourceFile());
const compile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const enums={};new Function('exports',compile([parse('packages/safe-cex/src/types.ts'),parse('packages/logging/src/LogSeverity.ts')].flatMap(ast=>ast.statements.filter(ts.isEnumDeclaration)).map(print).join('\n')))(enums);
const exchangeClass=parse('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts').statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='AsterDEXExchange');
const mapMethod=exchangeClass.members.find(n=>n.name?.getText()==='reverseMapOrderType');if(!mapMethod)throw Error('Missing actual reverseMapOrderType');
const mapExports={};new Function('exports',...Object.keys(enums),compile('export class SourceMapping { '+print(mapMethod)+' }'))(mapExports,...Object.values(enums));
const sourceMapping=new mapExports.SourceMapping();
const config=parse('packages/safe-cex/src/config/config.ts').statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText()==='ENABLE_RECONNECTING_WEBSOCKET');
if(!config||![ts.SyntaxKind.TrueKeyword,ts.SyntaxKind.FalseKeyword].includes(config.initializer.kind))throw Error('Config literal changed');
const reconnecting=config.initializer.kind===ts.SyntaxKind.TrueKeyword;
const clone={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/clone.ts'),'utf8')))(clone);
const compiler=path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)),'..'),dep=name=>require(path.join(path.dirname(compiler),name));
const storeExports={};new Function('exports','clone','virtualClock','uniqBy',compile(Object.values(enums).length? [parse('packages/safe-cex/src/types.ts'),parse('packages/logging/src/LogSeverity.ts')].flatMap(ast=>ast.statements.filter(ts.isEnumDeclaration)).map(print).join('\n')+'\n'+parse('packages/safe-cex/src/store/store.base.ts').statements.filter(n=>!ts.isImportDeclaration(n)).map(print).join('\n'):''))(storeExports,clone.clone,{getCurrentTimeMs:()=>1234},dep('lodash/uniqBy'));
let timers=new Map(),now=1000,nextId=0;
const originalSet=setTimeout,originalClear=clearTimeout,originalInterval=setInterval,originalClearInterval=clearInterval,originalNow=Date.now;
global.setTimeout=(fn,delay)=>{const id=++nextId;timers.set(id,{fn,delay});return id;};global.clearTimeout=id=>timers.delete(id);global.setInterval=global.setTimeout;global.clearInterval=global.clearTimeout;Date.now=()=>now;
class Base {
 constructor(parent){this.parent=parent;this.stats={};this.received=0;this.pongs=0;this.closes=0;this.baseDisposals=0;this.closeStates=[];this.onClose=()=>{this.closeStates.push(this.stateFlags());this.closes++;};}
 get isDisposed(){return this.parent.isDisposed;}
 stateFlags(){return ['listenKeyRefreshTimer','listenKeyWatchdogTimer','connectRetryTimer','positionRepairTimer','balanceReconcileTimer','orderReconcileTimer'].map(k=>this[k]!==undefined);}
 createWebsocket(url){this.parent.urls.push(url);if(this.parent.dialError)throw Error(this.parent.dialError);const handlers={};const ws={readyState:1,addEventListener:(key,fn)=>handlers[key]=fn,send:raw=>this.parent.wire.push(raw),terminate:()=>{},close:()=>{},handlers};return ws;}
 trackLastReceived(){this.received=now;}
 recordMessageHandlerStats(key){this.stats[key]=(this.stats[key]||0)+1;}
 handlePongEvent(){this.pongs++;}
 reconnect(){this.parent.reconnects++;}
 dispose(){this.baseDisposals++;this.ws=undefined;}
}
const privateExports=load('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts',[],{'@tealstreet/json-parse/src/jsonParse':load('packages/json-parse/src/jsonParse.ts',[],{}),'../../config/config':{ENABLE_RECONNECTING_WEBSOCKET:reconnecting},'../../types':enums,'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1234}},'../base.ws':{BaseWebSocket:Base},'./asterdex.types':values});
const order=(extra={})=>({id:'1',status:'open',symbol:'BTCUSDT',type:'limit',side:'buy',price:10,amount:5,filled:0,remaining:5,reduceOnly:false,lastUpdate:500,info:{tag:'retained'},...extra});
const position=(id,side,extra={})=>({id,symbol:'BTCUSDT',side,entryPrice:10,notional:20,leverage:3,unrealizedPnl:2,contracts:2,liquidationPrice:1,info:{tag:id},...extra});
const balance=(extra={})=>({total:100,free:80,used:20,upnl:7,info:{root:'retained'},currencies:{USDT:{total:100,free:80,used:20,upnl:7,info:{asset:'retained'}}},...extra});
const newData=(extra={})=>({e:'ORDER_TRADE_UPDATE',o:{i:1,X:'NEW',s:'BTCUSDT',o:'LIMIT',S:'BUY',p:'0',sp:'11',q:'5',z:'1',R:false,cp:true,...extra}});
const account=(E,a)=>({e:'ACCOUNT_UPDATE',E,a});
const updates=[{asset:'USDT',total:110,free:85,used:25}];
const cases=[
 {name:'construct-close-dispose',family:'lifecycle',initial:{orders:[order()],balance:balance()},ops:[{kind:'connect'},{kind:'schedule-all'},{kind:'close'},{kind:'dispose'}]},
 {name:'connect-production',family:'connection',ops:[{kind:'connect'},{kind:'open'}]},
 {name:'connect-testnet',family:'connection',testnet:true,ops:[{kind:'connect'}]},
 {name:'post-failure-retry',family:'connection',postError:'http down',ops:[{kind:'connect'}]},
 {name:'dial-failure-retry',family:'connection',dialError:'dial down',ops:[{kind:'connect'}]},
 {name:'message-routing',family:'dispatch',ops:[{kind:'message',raw:'invalid'},{kind:'message',raw:'null'},{kind:'message',raw:'0'},{kind:'message',raw:'{"id":42,"e":"ACCOUNT_UPDATE"}'},{kind:'message',raw:'{"id":"42","e":"unknown"}'},{kind:'message',raw:'{"e":"constructor"}'},{kind:'message',raw:'{"e":"__proto__"}'},{kind:'message',raw:'{"answer":1}'},{kind:'message',raw:'{"e":"outboundAccountPosition","B":[{"a":"USDT","f":"8","l":"2"}]}'},{kind:'message',raw:'{"e":"listenKeyExpired"}'}]},
 {name:'new-close-position',family:'orders',ops:[{kind:'message',data:newData()}]},
 {name:'partial-overfill',family:'orders',initial:{orders:[order()]},ops:[{kind:'message',data:newData({X:'PARTIALLY_FILLED',z:'7',l:'2',L:'12',ap:'10',T:99})}]},
 {name:'filled-removes-before-order-emission',family:'orders',initial:{orders:[order()]},ops:[{kind:'message',data:newData({X:'FILLED',z:'5',l:'5',L:'0',ap:'13',T:99})}]},
 {name:'nontrade-filled',family:'orders',initial:{orders:[order()]},ops:[{kind:'message',data:newData({X:'FILLED',z:'5',l:'0'})}]},
 {name:'cancel-preserves-fill',family:'orders',initial:{orders:[order({filled:2,remaining:3})]},ops:[{kind:'message',data:newData({X:'CANCELED',T:42})}]},
 {name:'unknown-status-fallback',family:'orders',initial:{orders:[order()]},ops:[{kind:'message',data:newData({X:'PENDING',q:'5',z:'2',T:0,E:55})}]},
 {name:'legacy-report',family:'orders',initial:{orders:[order()]},ops:[{kind:'message',data:{e:'executionReport',i:1,X:'PARTIALLY_FILLED',q:'5',z:'2'}}]},
 {name:'bad-numbers-native-nan',family:'orders',ops:[{kind:'message',data:newData({p:'bad',sp:'bad',q:'bad',z:'bad'})}]},
 {name:'balance-single-asset',family:'balance',initial:{balance:balance()},ops:[{kind:'balance',updates,deltas:[['USDT',3]]}]},
 {name:'balance-first-collateral',family:'balance',initial:{balance:balance({currencies:undefined})},ops:[{kind:'balance',updates,deltas:[['USDT',3]]}]},
 {name:'balance-multiasset-retains-root',family:'balance',initial:{balance:balance()},ops:[{kind:'balance',updates:[...updates,{asset:'BTC',total:2,free:1,used:1}],deltas:[['USDT',3],['BTC',4]]}]},
 {name:'balance-repeated-asset',family:'balance',initial:{balance:balance()},ops:[{kind:'balance',updates:[...updates,{asset:'USDT',total:120,free:90,used:30}],deltas:[['USDT',3]]}]},
 {name:'account-updates-before-balance',family:'account',initial:{balance:balance(),positions:[position('long','long')]},ops:[{kind:'message',data:account(100,{B:[{a:'USDT',wb:'110',cw:'85'}],P:[{s:'BTCUSDT',ps:'BOTH',pa:'3',ep:'12',up:'5',cr:'4'}]})}]},
 {name:'account-tiny-existing-quantity',family:'account',initial:{balance:balance(),positions:[position('long','long')]},ops:[{kind:'message',data:account(110,{P:[{s:'BTCUSDT',ps:'BOTH',pa:'1e-8',ep:'12',up:'5',cr:'4'}]})}]},
 {name:'account-flip-removes-opposite',family:'account',initial:{balance:balance(),positions:[position('long','long'),position('other','long',{symbol:'ETHUSDT'})]},ops:[{kind:'message',data:account(101,{P:[{s:'BTCUSDT',ps:'BOTH',pa:'-3',ep:'12',up:'5',cr:'4'}]})}]},
 {name:'zero-removes-both-hedges',family:'account',initial:{balance:balance(),positions:[position('long','long'),position('short','short'),position('other','long',{symbol:'ETHUSDT'})]},ops:[{kind:'message',data:account(102,{P:[{s:'BTCUSDT',ps:'LONG',pa:'0',up:'0'}]})}]},
 {name:'account-stale-invalid',family:'account',initial:{balance:balance()},ops:[{kind:'message',data:account(103,{B:[{a:'USDT',wb:'120',cw:'90'}]})},{kind:'message',data:account(103,{B:[{a:'USDT',wb:'999',cw:'99'}]})},{kind:'message',data:account('104',{})},{kind:'message',data:account(105,{P:[{s:'BTCUSDT',pa:'bad',up:'0'}]})},{kind:'message',data:account(106,null)}]},
 {name:'account-multiasset-reconcile',family:'account',initial:{balance:balance()},ops:[{kind:'message',data:account(104,{B:[{a:'BTC',wb:'2',cw:'1'}],P:[]})}]},
 {name:'account-missing-payload-parts',family:'account',initial:{balance:balance()},ops:[{kind:'message',data:account(105,{})}]},
];
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
(async()=>{const all=[];try{for(const c of cases){timers=new Map();now=1000;const store=new storeExports.DefaultStore();store.update(structuredClone(c.initial||{}));let pv=0,bv=0,notifications=0;store.subscribe(()=>notifications++);const logs=[],events=[],urls=[],wire=[],requests=[],closeStates=[],results=[];
 const parent={isDisposed:false,options:{key:'',secret:'',testnet:c.testnet||false},store,urls,wire,reconnects:0,dialError:c.dialError,log:(message,severity)=>logs.push({message,severity}),getFapiPath:e=>'/fapi/v3/'+e,primaryAPI:{post:p=>{requests.push(['POST',p]);return c.postError?Promise.reject(Error(c.postError)):Promise.resolve({data:{listenKey:'native-key'}});},put:()=>Promise.resolve({})},emitter:{emit:(event,...args)=>events.push({event,args:structuredClone(args),...(event==='account'?{balanceAtAccount:structuredClone(store.balance)}:{})})},reverseMapOrderType:type=>sourceMapping.reverseMapOrderType(type),markBalanceStateUpdatedByWebsocket:()=>++bv,markPositionStateUpdatedByWebsocket:()=>++pv,getBalanceStateVersion:()=>bv,getPositionStateVersion:()=>pv,fetchAndSetPrivateSnapshot:n=>requests.push(['SNAPSHOT',n]),fetchPositions:()=>Promise.resolve([]),fetchOrders:()=>Promise.resolve([]),reconcileBalanceAfterPrivateUpdate:()=>Promise.resolve(true)};
 const s=new privateExports.AsterDEXPrivateWebsocket(parent,store);const handlerKeys=Object.keys(s.messageHandlers),snapshots=[];
 const snapshot=()=>({flags:s.stateFlags(),key:s.listenKey,retry:s.connectRetryCount,last:s.lastAccountUpdateEventTime,pv,bv,orders:structuredClone(store.orders),positions:structuredClone(store.positions),balance:structuredClone(store.balance)});
 snapshots.push(snapshot());
 for(const op of c.ops){switch(op.kind){case'connect':await s.connectAndSubscribe();break;case'open':s.ws.handlers.open();break;case'message':s.onMessage({data:op.raw??JSON.stringify(op.data)});break;case'balance':results.push(s.applyBalanceUpdates(op.updates,new Map(op.deltas)));break;case'schedule-all':s.scheduleConnectRetry();s.schedulePositionRepair();s.scheduleBalanceReconcile(bv);s.scheduleOrderReconcile();break;case'close':s.onClose({code:1000});break;case'dispose':s.dispose();break;default:throw Error('Unknown operation');}await flush();snapshots.push(snapshot());}
 all.push({...c,initial:c.initial||{},handlerKeys,snapshots,logs,events,requests,urls,wire,results,received:s.received,pongs:s.pongs,closes:s.closes,closeStates:s.closeStates,baseDisposals:s.baseDisposals,notifications,reconnecting});
 }}finally{global.setTimeout=originalSet;global.clearTimeout=originalClear;global.setInterval=originalInterval;global.clearInterval=originalClearInterval;Date.now=originalNow;}let selected=all;
 if(process.argv.includes('--boundaries')) {
  const store=new storeExports.DefaultStore();store.update({balance:balance()});const logs=[];const parent={isDisposed:false,store,log:(message,severity)=>logs.push({message,severity}),options:{},emitter:{emit:()=>{}},reverseMapOrderType:type=>sourceMapping.reverseMapOrderType(type)};const s=new privateExports.AsterDEXPrivateWebsocket(parent,store);
  const reconcile=s.applyBalanceUpdates([{asset:'__proto__',total:2,free:1,used:1}],new Map());
  const prefix=JSON.stringify({e:'unknown',text:''}).indexOf('"text":"')+'"text":"'.length;
  s.onMessage({data:JSON.stringify({e:'unknown',text:'x'.repeat(199-prefix)+'😀'})});
  s.handleOrderTradeUpdate({o:{i:9,X:'NEW',s:'BTCUSDT',o:'UNRECOGNIZED',S:'BUY',q:'1',z:'0'}});
  const message=logs.find(l=>l.message.startsWith('Unhandled private event type')).message;
  selected={currencyKeys:Object.keys(store.balance.currencies),prototypeTotal:Object.getPrototypeOf(store.balance.currencies).total,reconcile,lastLogUnit:message.charCodeAt(message.length-1),orderType:store.orders[0].type};s.dispose();
 }
 process.stdout.write(JSON.stringify(selected,(_,v)=>typeof v==='number'&&!Number.isFinite(v)?{number:String(v)}:v));})().catch(e=>{process.stderr.write(e.stack);process.exitCode=1;});
