const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const methods = {};
for (const [scope,names] of Object.entries({exchange:['start','tick','listenOHLCV'],public:['connectAndSubscribe'],futures:['connectAndSubscribe'],private:['connectAndSubscribe','onOpen']})) {
 const file=path.join(root,`packages/safe-cex/src/exchanges/bitunix/bitunix.${scope==='exchange'?'exchange':'ws-'+scope}.ts`);
 const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(ts.isClassDeclaration);
 for(const name of names){const n=cls.members.filter(n=>n.name?.getText(ast)===name);assert.equal(n.length,1);methods[scope+'.'+name]=ts.transpileModule('(function(){return ('+n[0].initializer.getText(ast)+');})',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;}
}
const typeFile=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts');
const typeAst=ts.createSourceFile(typeFile,fs.readFileSync(typeFile,'utf8'),ts.ScriptTarget.Latest,true);
const initializer=typeAst.statements.flatMap(s=>s.declarationList?.declarations??[]).find(d=>d.name.getText(typeAst)==='BASE_WS_URL').initializer.getText(typeAst);
const baseUrls=vm.runInNewContext('('+initializer+')');
function num(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return {bits:Number.isNaN(n)?'7ff8000000000000':b.toString('hex')};}
function obs(v){if(v===undefined)return {undefined:true};if(typeof v==='number')return num(v);if(typeof v==='string')return {units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(Array.isArray(v))return v.map(obs);if(v&&typeof v==='object'){if(v.message&&v.name)return {error:[v.name,v.message]};return {entries:Object.entries(v).map(([k,v])=>[k,obs(v)])};}return v;}
const rows=[];
async function exchange(key,f){
 const events=[];let disposed=!!f.disposed,loaded={seed:'kept',orders:false},lastMode=f.lastMode??'HEDGE',mode=f.mode??'HEDGE';
 const values={fetchMarkets:[{tag:'m0'},{tag:'m1'}],fetchTickers:[{tag:'t0'}],fetchOrders:[{tag:'o0'}],fetchPositions:[{tag:'p0'}],fetchPositionMode:true};
 const note=(name,...args)=>{events.push([name,...args.map(obs)]);if(f.fail===name)throw new Error(name+' failed');};
 const owner={
  initExternalPublicData:async()=>{note('external');return !!f.external;},
  log:(s)=>note('log',s),
  configurePublicWebsocketShards:(markets)=>{assert.equal(markets,values.fetchMarkets);note('shards',markets);return [{connectAndSubscribe:()=>note('shard','a')},{connectAndSubscribe:()=>note('shard','b')}];},
  tick:async()=>{note('initialTick');if(f.disposeAfter==='initialTick')disposed=true;},
  emitAndLogError:(s)=>note('report',s),getTickInterval:()=>{note('interval');return 1234;},
  publicWebsocket:{},futuresWebsocket:{connectAndSubscribe:()=>note('connect','futuresWebsocket')},privateWebsocket:{connectAndSubscribe:()=>note('connect','privateWebsocket')},
  store:{update:(patch)=>{for(const name of ['markets','tickers','orders','positions'])if(name in patch)assert.equal(patch[name],values['fetch'+name[0].toUpperCase()+name.slice(1)]??balancePositions);note('update',patch);if(patch.loaded)loaded=patch.loaded;},setSetting:(name,value)=>note('setting',name,value)},
 };
 Object.defineProperty(owner.store,'loaded',{get:()=>{note('loaded');return loaded;}});
 for(const name of ['isDisposed','isPublicDataOnly'])Object.defineProperty(owner,name,{get:()=>{const v=name==='isDisposed'?disposed:!!f.publicOnly;note(name,v);return v;}});
 Object.defineProperty(owner,'lastTickAccountPositionMode',{get:()=>{note('lastMode',lastMode);return lastMode;},set:v=>{note('setLastMode',v);lastMode=v;}});
 Object.defineProperty(owner,'accountPositionMode',{get:()=>{note('mode',mode);return mode;}});
 for(const [name,value] of Object.entries(values))owner[name]=async()=>{note(name);if(f.yield===name)await Promise.resolve();if(f.disposeAfter===name)disposed=true;return value;};
 const balance={tag:'balance'},balancePositions=[{tag:'balance-position'}];
 owner.fetchBalance=async()=>{note('fetchBalance');if(f.fetchFailure)throw new Error('balance failed');if(f.disposeAfter==='fetchBalance')disposed=true;values.fetchPositions=balancePositions;return {balance,positions:balancePositions};};
 const context={loop:(fn,delay)=>{note('loop',delay);if(f.invokeLoop){f.invokeLoop=false;fn();}}};
 owner.method=vm.runInNewContext(methods[key],context).call(owner);
 let result;try{await owner.method(f.count??0);result={ok:true};}catch(e){result={error:obs(e)};}
 rows.push({key,fixture:f,events,result,state:{disposed,loaded:obs(loaded),lastMode:obs(lastMode)}});
}
async function connection(key,f){
 const events=[],listeners={};let timer=77,socket,closed=!!f.closed,fallbackReads=0;
 const state={isDisposed:!!f.disposed,connectionAttempts:f.attempts??0,MAX_ATTEMPTS:2,lastMessageCount:f.last??0,messageCount:f.count??0,useFallbackUrl:!!f.fallback,testnet:!!f.testnet};
 const note=(name,...args)=>{events.push([name,...args.map(obs)]);if(f.fail===name)throw new Error(name+' failed');};
 const owner={clearSubscriptionQueue:()=>note('queue'),onOpen(){},onMessage(){},onClose:()=>note('onClose'),createWebsocket:url=>{note('create',url);return {addEventListener:(event,fn)=>{note('listener',event);listeners[event]=fn;},get readyState(){note('readyState');return closed?3:1;},close:()=>{note('closeSocket');if(f.closeThrows)throw new Error('close failed');closed=true;}}},parent:{name:'bitunix',options:{},emitter:{emit:(...args)=>note('proxy',...args)}}};
 for(const field of Object.keys(state))Object.defineProperty(field==='testnet'?owner.parent.options:owner,field,{get:()=>{const v=field==='useFallbackUrl'&&f.useReads ? f.useReads[fallbackReads++]??state[field]:state[field];note('get',field,v);return v;},set:value=>{note('set',field,value);state[field]=value;}});
 Object.defineProperty(owner,'ws',{get:()=>{note('socket',socket?true:false);return socket;},set:v=>{note('setSocket');socket=v;}});
 Object.defineProperty(owner,'pingLoopTimeoutId',{get:()=>{note('timer',timer);return timer;}});
 const scope=key.split('.')[0],routeKey={public:'wsPublic',futures:'wsBusiness',private:'wsPrivate'}[scope],baseKey={public:'public',futures:'frontendfutures',private:'private'}[scope];
 const context={WebSocket:{CLOSED:3},clearTimeout:t=>note('clear',t),resolveTransportRouting:()=>{note('routing',routeKey);return {[routeKey]:{fallback:f.urlUnits?String.fromCharCode(...f.urlUnits):(f.url??'')}};},BASE_WS_URL:{[baseKey]:new Proxy(baseUrls[baseKey],{get:(obj,key)=>{note('base',baseKey,key);return obj[key];}})}};
 owner.method=vm.runInNewContext(methods[key],context).call(owner);
 let result;try{await owner.method();if(f.noSocket)socket=undefined;if(f.event)listeners[f.event]();result={ok:true};}catch(e){result={error:obs(e)};}
 rows.push({key,fixture:f,events,result,state:obs(state)});
}
async function privateOpen(f){
 const key='private.onOpen',events=[],timers=[];let disposed=!!f.disposed,timer=f.timer??0,pings=0,resolve,reject;
 const note=(name,...args)=>{events.push([name,...args.map(obs)]);if(f.fail===name)throw new Error(name+' failed');};
 const owner={ping:()=>{note('ping');pings++;if(pings===f.disposePing)disposed=true;},sendLogin:()=>{note('login');return new Promise((res,rej)=>{resolve=res;reject=rej;});}};
 Object.defineProperty(owner,'isDisposed',{get:()=>{note('disposed',disposed);return disposed;}});
 Object.defineProperty(owner,'pingLoopTimeoutId',{get:()=>{note('timer',timer);return timer;},set:v=>{note('setTimer',v);timer=v;}});
 const unhandled=e=>note('unhandled',e);process.on('unhandledRejection',unhandled);
 owner.method=vm.runInNewContext(methods[key],{clearTimeout:t=>note('clear',t),setTimeout:(fn,delay)=>{note('schedule',delay);timers.push(fn);return 99;}}).call(owner);
 let result;try{owner.method();result={ok:true};}catch(e){result={error:obs(e)};}
 if(resolve){note('resume');if(f.disposeResume)disposed=true;if(f.rejectLogin)reject(new Error('login rejected'));else resolve();}
 await new Promise(res=>setImmediate(res));
 if(f.invokeTimer&&timers.length){if(f.disposeTimer)disposed=true;try{timers[0]();}catch(e){result={error:obs(e)};}}
 process.removeListener('unhandledRejection',unhandled);
 rows.push({key,fixture:f,events,result,state:{disposed,timer:num(timer),pings:num(pings)}});
}
async function delegate(f){
 const key='exchange.listenOHLCV',events=[],opts={symbol:'BTCUSDT'},callback={tag:'callback'},unlisten={tag:'unlisten'};
 const note=(name,...args)=>{events.push([name,...args.map(obs)]);if(f.fail===name)throw new Error(name+' failed');};
 const owner={symbolSettings:{get:s=>{note('configured',s);return f.configured?{}:undefined;}},updateSymbolMetadata:s=>note('metadata',s),futuresWebsocket:{listenOHLCV:(o,c)=>{assert.equal(o,opts);assert.equal(c,callback);note('listen');return unlisten;}}};
 Object.defineProperty(opts,'symbol',{get:()=>{note('symbol');if(f.fail==='secondSymbol'&&events.filter(e=>e[0]==='symbol').length===2)throw new Error('secondSymbol failed');return 'BTCUSDT';}});
 owner.method=vm.runInNewContext(methods[key],{}).call(owner);
 let result;try{assert.equal(owner.method(opts,callback),unlisten);result={ok:true};}catch(e){result={error:obs(e)};}
 rows.push({key,fixture:f,events,result});
}
(async()=>{
 for(const scope of ['public','futures','private'])for(const f of [{},{disposed:true},{attempts:2},{attempts:2,url:'wss://proxy'},{attempts:2,last:1,url:'wss://proxy'},{attempts:2,count:1},{fallback:true,url:'wss://proxy',testnet:true},{fallback:true},{fallback:true,url:'wss://proxy',fail:'proxy'},{event:'close'},{event:'error'},{event:'error',closed:true},{event:'error',noSocket:true},{event:'error',closeThrows:true},{fail:'create'},{fail:'listener'},{fail:'queue'},{fail:'routing'},{fallback:true,urlUnits:[119,115,115,58,47,47,0xd800]},{url:'wss://proxy',useReads:[false,false,true]}])await connection(scope+'.connectAndSubscribe',f);
 for(const f of [{},{external:true},{publicOnly:true},{external:true,publicOnly:true},{disposeAfter:'fetchMarkets'},{disposeAfter:'fetchTickers'},{disposeAfter:'initialTick'},{disposeAfter:'fetchOrders'},{yield:'fetchMarkets'},{fail:'fetchMarkets'},{fail:'fetchPositionMode'},{fail:'shard'}])await exchange('exchange.start',f);
 for(const f of [{},{count:1},{count:32},{count:1,mode:'ONE_WAY'},{disposed:true},{publicOnly:true},{disposeAfter:'fetchOrders'},{disposeAfter:'fetchBalance'},{fail:'fetchPositions'},{fail:'fetchOrders'},{count:1,fail:'fetchBalance'},{fail:'update'},{fail:'report',fetchFailure:true},{fail:'interval'},{count:-32},{count:33,yield:'fetchPositions',mode:'ONE_WAY'}])await exchange('exchange.tick',f);
 for(const f of [{},{disposed:true},{timer:77},{timer:0},{disposePing:1},{disposeResume:true},{rejectLogin:true},{invokeTimer:true},{invokeTimer:true,disposeTimer:true},{fail:'login'},{fail:'ping'},{fail:'clear',timer:77}])await privateOpen(f);
 for(const f of [{},{configured:true},{fail:'metadata'},{fail:'configured'},{fail:'listen'},{fail:'secondSymbol'}])await delegate(f);
 assert.equal(rows.length,106);
 process.stdout.write(JSON.stringify({rows}));
})().catch(e=>{console.error(e);process.exit(1);});
