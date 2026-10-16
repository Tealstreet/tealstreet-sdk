const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function execute(relative,imports){const code=ts.transpileModule(fs.readFileSync(path.join(root,relative),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const m={exports:{}};vm.runInThisContext('(function(require,module,exports){'+code+'\n})')(n=>{assert(Object.hasOwn(imports,n),n);return imports[n]},m,m.exports);return m.exports;}
const severity=execute('packages/logging/src/LogSeverity.ts',{}).LogSeverity;
const enums=execute('packages/safe-cex/src/types.ts',{'@tealstreet/logging/src/LogSeverity':{LogSeverity:severity}});
const constants=execute('packages/safe-cex/src/exchanges/phemex/phemex.types.ts',{'../../types':enums});
const file=path.join(root,'packages/safe-cex/src/exchanges/phemex/phemex.ws-public.ts'),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.ES2022,true),cls=ast.statements.find(n=>ts.isClassDeclaration(n));
const names=['connectAndSubscribe','onOpen','subscribe','updateSubscriptionLimitWarning','ping','dispatchTopic','getKlineInterval','onMessage'];
const fields=['MAX_SUBSCRIPTIONS','SUBSCRIPTION_WARNING_THRESHOLD'].map(name=>cls.members.find(n=>n.name?.getText(ast)===name).getText(ast)).join('\n');
const body=names.map(name=>{const a=cls.members.filter(n=>n.name?.getText(ast)===name);assert.equal(a.length,1);return a[0].getText(ast)}).join('\n');
function decode(v){if(v&&v.$units)return String.fromCharCode(...v.$units);if(v&&v.$bits)return Buffer.from(v.$bits,'hex').readDoubleBE();if(v&&v.$undefined)return undefined;if(Array.isArray(v))return v.map(decode);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,decode(v)]));return v;}
function obs(v){if(v===undefined)return {undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {bits:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')}}if(typeof v==='string')return {units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(Array.isArray(v))return v.map(obs);if(v&&typeof v==='object')return {object:Object.entries(v).map(([k,v])=>[obs(k),obs(v)]).sort((a,b)=>JSON.stringify(a[0])<JSON.stringify(b[0])?-1:JSON.stringify(a[0])>JSON.stringify(b[0])?1:0)};return v;}
const encoded=s=>({$units:Array.from({length:s.length},(_,i)=>s.charCodeAt(i))}),nan={$bits:'7ff8000000000000'},inf={$bits:'7ff0000000000000'},negzero={$bits:'8000000000000000'};
const topics=n=>Array.from({length:n},(_,i)=>['topic'+i,1]);
const cases={
 connect:[{}, {disposed:true},{testnet:true},{failure:'create'},{failure:'listener:message'}],
 open:[{}, {disposed:true},{failure:'send'},{failure:'reset'},{replaceSend:true}],
 subscribe:[{}, {topics:[['orderbook_p.subscribe|||BTCUSDT',1],['kline_p.subscribe|||BTCUSDT|||60',1]]},{topics:[['10',0],['2',-1],['plain|||a|||b',2]]},{topics:[['kline_p.subscribe',1],['kline_p.subscribe|||X|||wat',1],['kline_p.subscribe|||X|||0x10',1]]},{topics:[['kline_p.subscribe|||\ud800|||Infinity',1],['plain|||\udfff|||',1]]},{topics:[['kline_p.subscribe|||X|||60',1]],replaceSend:true},{topics:[['a',1],['b',1]],failure:'send'}],
 warning:[{}, {topics:topics(16)},{topics:topics(17)},{topics:topics(19)},{topics:topics(17),warned:true},{topics:[['a',0],['b',-1],['c',nan],['d',inf]],warned:true},{topics:topics(17),repeat:true},{topics:topics(17),failure:'warning'}],
 ping:[{}, {disposed:true},{requestId:negzero},{requestId:inf},{requestId:9007199254740992,repeat:true},{failure:'send'}],
 dispatch:[{callbacks:[]},{callbacks:['a','b']},{callbacks:['a','b'],each:true},{callbacks:['a','b'],failure:'callback:a'},{callbacks:['a'],append:true},{callbacks:['a','b'],remove:true},{callbacks:['a','b'],replace:true},{topic:encoded('\ud800'),callbacks:['a'],payload:{x:3}}],
 interval:[{resolutions:[60,'60',300,99]},{resolutions:['Infinity',nan,null,'',negzero],timeframes:[['zero',0],['nan',nan]]},{resolutions:[1,1],timeframes:[['first','1'],['last',1]],mutateTimeframes:true},{resolutions:['0x10',true,' 60 '],timeframes:[['sixteen',16],['minute',60]]},{resolutions:[{}]},{resolutions:[60],failure:'timeframe'},{resolutions:[60],timeframes:[[encoded('\ud800'),60]]},{resolutions:[60],cacheReplacesFrames:true}],
 message:[{frame:null},{disposed:true,frame:{method:'server.ping'}},{frame:{method:'server.ping',result:'x'}},{frame:{result:'pong',kline_p:[]}},{frame:{symbol:'BTC',kline_p:[[1,60],[2,'60'],[3,300],{},[4,99],[5,60]]}},{frame:{symbol:'',kline_p:[[1,60]]}},{frame:{symbol:encoded('\ud800'),orderbook_p:{bids:[]}}},{frame:{symbol:8,kline_p:[],method:'m'}},{frame:{symbol:'BTC',kline_p:[],orderbook_p:{},method:'m'}},{frame:{method:'m',data:[1]}},{frame:{method:3}},{raw:'invalid'},{frame:{symbol:'BTC',kline_p:{} }},{frame:{method:'server.ping'},failure:'parse'},{frame:{symbol:'BTC',kline_p:[[1,60]]},failure:'dispatch'}]
};
function run(name,encodedInput){const input=decode(encodedInput),effects=[],error={marker:'original'};let w;
const effect=(name,...args)=>{effects.push([name,...args]);if(input.failure===name)throw error};
class Base{}
const actualParse=execute('packages/json-parse/src/jsonParse.ts',{}).jsonParse;
const jsonParse=data=>{effect('parse');return actualParse(data)};
const stringify=request=>{effect('serialize',request);if(input.replaceSend)w.send=()=>{throw error};return JSON.stringify(request)};
const context=vm.createContext({Base,BASE_WS_URL:constants.BASE_WS_URL,LogSeverity:severity,performance:{now:()=>123.25},jsonParse,JSON:{stringify}});
const C=vm.runInContext(ts.transpileModule('class BinancePublicWebsocket extends Base {'+fields+body+'}\nBinancePublicWebsocket',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText,context);
w=new C();w.isDisposed=!!input.disposed;w.topics=Object.fromEntries(input.topics||[]);w.hasWarnedSubscriptionLimit=!!input.warned;w.requestId=input.requestId??1200;w.parent={options:{testnet:!!input.testnet},timeframes:Object.fromEntries(input.timeframes||[['1m',60],['5m',300]]),log:(m,s)=>{effect('warning',m,s)}};
w.createWebsocket=url=>{effect('create',url);return {addEventListener:(event,member)=>effect('listener:'+event,member===w.onOpen?'onOpen':member===w.onMessage?'onMessage':'onClose')}};w.onClose=()=>{};
w.send=text=>effect('send',text);w.resetOrderbook=()=>effect('reset');if(name!=='subscribe')w.subscribe=()=>effect('subscribe');if(name!=='ping')w.ping=()=>effect('ping');
w.trackLastReceived=()=>effect('received');w.handlePongEvent=()=>effect('pong');w.recordMessageHandlerStats=topic=>effect('stats',topic);
if(input.failure==='timeframe'){Object.defineProperty(w.parent.timeframes,'1m',{get(){effect('timeframe')}})}
if(input.cacheReplacesFrames){let cache;Object.defineProperty(w,'klineIntervalByResolution',{get(){return cache},set(v){cache=v;w.parent.timeframes={other:60}}})}
w.messageHandlers={};const callbacks=(input.callbacks||[]).map(id=>{const f=payload=>{effect('callback:'+id,payload);if(id==='a'&&input.append)w.messageHandlers.topic.push(p=>effect('callback:c',p));if(id==='a'&&input.remove)w.messageHandlers.topic.pop();if(id==='a'&&input.replace)w.messageHandlers.topic=[p=>effect('callback:z',p)]};return f});w.messageHandlers[input.topic??'topic']=callbacks;
if(name==='message')w.dispatchTopic=(...args)=>effect('dispatch',...args);
let result;let thrown;
try{if(name==='connect')w.connectAndSubscribe();else if(name==='open')w.onOpen();else if(name==='subscribe')w.subscribe();else if(name==='warning'){w.updateSubscriptionLimitWarning();if(input.repeat)w.updateSubscriptionLimitWarning()}else if(name==='ping'){w.ping();if(input.repeat)w.ping()}else if(name==='dispatch')w.dispatchTopic(input.topic??'topic',input.payload??{payload:1},!!input.each);else if(name==='interval'){result=[];for(const r of input.resolutions){result.push(w.getKlineInterval(r));if(input.mutateTimeframes)w.parent.timeframes={'changed':1}}}else w.onMessage({data:input.raw??JSON.stringify(input.frame)})}catch(e){thrown=e===error?'original':'intrinsic'}
return {input:encodedInput,expected:obs({effects,result,thrown,warned:w.hasWarnedSubscriptionLimit,requestId:w.requestId})};}
function pack(v){if(typeof v==='string')return encoded(v);if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object'&&!v.$units&&!v.$bits)return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,pack(v)]));return v}
process.stdout.write(JSON.stringify(cases[process.argv[2]].map(input=>run(process.argv[2],pack(input)))));
