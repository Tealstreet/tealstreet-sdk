const {ts,fs,path,vm,root,assert}=require('./oracle-loader.cjs');
function ast(scope){const f=path.join(root,`packages/safe-cex/src/exchanges/bitunix/bitunix.ws-${scope}.ts`);return ts.createSourceFile(f,fs.readFileSync(f,'utf8'),ts.ScriptTarget.Latest,true);}
const futures=ast('futures'),member=futures.statements.find(ts.isClassDeclaration).members.find(n=>n.name?.getText(futures)==='listenOHLCV');
const method=ts.transpileModule('(function(){return ('+member.initializer.getText(futures)+');})',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const publicAst=ast('public'),intervalNode=publicAst.statements.flatMap(n=>n.declarationList?.declarations??[]).find(n=>n.name.getText(publicAst)==='INTERVAL');
const INTERVAL=vm.runInNewContext('('+intervalNode.initializer.getText(publicAst)+')');
function bits(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return {bits:Number.isNaN(n)?'7ff8000000000000':b.toString('hex')};}
function obs(v){if(v===undefined)return {undefined:true};if(typeof v==='number')return bits(v);if(typeof v==='string')return {units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(Array.isArray(v))return v.map(obs);if(v&&typeof v==='object'){if(v.name&&v.message)return {error:[v.name,v.message]};return {entries:Object.entries(v).map(([k,v])=>[k,obs(v)])};}return v;}
const raw={data:{time:1700000000000,open:'-0',high:'2e2x',low:'  .5',close:'123.4tail',volume:'Infinity'}};
const fixtures=[...Object.keys(INTERVAL).map(interval=>({interval})),{},{symbol:'ΩΣ'},{symbol:'İBTC'},{symbol:'😀BTC'},{interval:'unknown'},{connected:false,actions:['connect','retry','parse','cancel']},{connected:false,actions:['cancel','connect','retry','parse','cancel']},{ticker:false},{topicCount:-1},{topicCount:0},{nanCount:true},{actions:['parse','cancel','cancel']},{second:true},{mutate:'ETHUSDT'},{raw:{data:{...raw.data,time:'123'}}},{raw:{data:{...raw.data,time:'bad'}}},{raw:{data:{...raw.data,open:null,high:['1','2'],low:{},close:'',volume:'-Infinity'}}},{raw:null,boundary:'Cannot read candle data from null'},{raw:{},boundary:'Missing candle data'},{raw:{data:null},boundary:'Cannot read candle fields from null'},...['send','setHandlers','callback','updateLast','find'].map(fail=>({fail}))];
assert.equal(fixtures.length,37);
const rows=[];
for(const f of fixtures){
 const events=[],names=new WeakMap(),waits=[];let next=0,connected=f.connected!==false;
 const note=(name,...args)=>{events.push([name,...args.map(obs)]);if(f.fail===name)throw new Error(name==='callback'?'callback failed':name+' failed');};
 const opts={symbol:f.symbol??'BTCUSDT',interval:f.interval??'1m'},originalSymbol=opts.symbol;
 const handler=`market_${opts.symbol.toLowerCase()}_klineop_${INTERVAL[opts.interval]}`;
 const arg={event:'sub',params:{channel:handler}},topic=JSON.stringify(arg);
 const other=()=>{};names.set(other,'other');
 const handlers=f.existing?[other]:undefined,handlerMap=handlers?{[handler]:handlers}:{},topicMap={};
 if(f.topicCount!==undefined)topicMap[topic]=f.topicCount;if(f.nanCount)topicMap[topic]=NaN;
 function ids(v){return v===undefined?undefined:v.map(fn=>{if(!names.has(fn))names.set(fn,'p'+next++);return names.get(fn);});}
 const ticker={symbol:originalSymbol},tickers=f.ticker===false?[]:[ticker];let last;
 const owner={messageHandlers:new Proxy(handlerMap,{get:(o,k)=>{note('handlers',k,ids(o[k]));return o[k];},set:(o,k,v)=>{note('setHandlers',k,ids(v));o[k]=v;return true;}}),topics:new Proxy(topicMap,{get:(o,k)=>{note('count',k,o[k]);return o[k];},set:(o,k,v)=>{note('setCount',k,v);o[k]=v;return true;}}),send:s=>note('send',JSON.parse(s)),cleanupMessageHandlerStats:h=>note('cleanup',h),store:{tickers,updateTicker:(t,patch)=>{assert.equal(t,ticker);note('updateLast',patch.last);last=patch.last;}}};
 tickers.find=function(fn){note('find',opts.symbol);return Array.prototype.find.call(this,fn);};
 Object.defineProperty(owner,'isConnected',{get:()=>{note('connected',connected);return connected;}});
 owner.method=vm.runInNewContext(method,{INTERVAL,setTimeout:(fn,delay)=>{note('wait',delay);waits.push(fn);return 99;}}).call(owner);
 let result,sourceCandles=[],nativeTimestamps=[];
 function callback(id){return c=>{sourceCandles.push(obs(c));nativeTimestamps.push(bits(Number(c.timestamp)));note('callback',id,c);};}
 try{
  const cancel=owner.method(opts,callback(0));
  if(f.second)owner.method(opts,callback(1));
  if(f.mutate)opts.symbol=f.mutate;
  for(const action of f.actions??['parse','cancel']){
   if(action==='connect')connected=true;
   else if(action==='retry')waits.shift()?.();
   else if(action==='cancel')cancel();
   else if(action==='parse')for(const parser of [...(handlerMap[handler]??[])])parser(f.raw===undefined?raw:f.raw);
  }
  result={ok:true};
 }catch(e){result={error:obs(e)};}
 rows.push({fixture:f,events,result,handler,topic,sourceCandles,nativeTimestamps,state:{handlers:ids(handlerMap[handler]),count:obs(topicMap[topic]),last:obs(last)}});
}
process.stdout.write(JSON.stringify({rows}));
