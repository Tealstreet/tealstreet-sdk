const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require},{filename:file});return exports;}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
const parsing=run(path.join(root,'packages/json-parse/src/jsonParse.ts'),id=>unused(id));
class Base {constructor(parent){this.parent=parent;this.isDisposed=false;}get store(){return this.parent.store;}}
let parseContext,throttleRecord;
function parseWire(wire){const c=parseContext;c.calls.push('parse');if(c.mode==='parse')throw c.errors.primary;const value=parsing.jsonParse(wire);if(c.mode==='captureDispatch'){c.payload=value.data;c.dataReads=0;Object.defineProperty(value,'data',{get(){c.calls.push('data');c.dataReads++;if(c.dataReads===2)Object.defineProperty(c.socket,c.dispatchName,{configurable:true,value(){throw Error('wrong captured handler');}});return c.payload;}});}return c.parsed=value;}
const Public=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-public.ts'),id=>{if(id==='lodash/throttle')return (callback,wait,opts)=>{if(throttleRecord)throttleRecord.push({wait:bits(wait),leading:opts.leading,trailing:opts.trailing});return callback;};if(id==='@tealstreet/json-parse/src/jsonParse')return {jsonParse:parseWire};if(id==='../../types')return types;if(id==='./backpack.types')return constants;if(id==='../base.ws')return {BaseWebSocket:Base};return unused(id);}).BackpackPublicWebSocket;
const Private=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts'),id=>{if(id==='@tealstreet/json-parse/src/jsonParse')return {jsonParse:parseWire};if(id==='../../types')return types;if(id==='./backpack.types')return constants;if(id==='../base.ws')return {BaseWebSocket:Base};return unused(id);}).BackpackPrivateWebSocket;
function context(mode){const c={mode,calls:[],messages:[],errors:{}};for(const [tag,text] of [['primary','primaryλ\ud800'],['lookup','emitter-failure'],['format','format-failure'],['emit','emit-failure']])c.errors[tag]={[Symbol.toPrimitive](){c.calls.push('format');if(mode==='format'||mode==='dispatchFormat')throw c.errors.format;return text;}};c.parent={};Object.defineProperty(c.parent,'emitAndLogError',{get(){c.calls.push('emitter');if(mode==='emitLookup')throw c.errors.lookup;return message=>{c.calls.push('emit');c.messages.push(units(message));if(mode==='emit'||mode==='dispatchEmit')throw c.errors.emit;};}});return c;}
function outcome(c,fn){try{fn();return 'ok';}catch(e){const tag=Object.entries(c.errors).find(([,error])=>error===e)?.[0];assert.ok(tag,'unrecognized error');return tag;}}
const wires=['null','false','0','""','[]','{}','{"result":null,"id":1,"e":"trade"}','{"result":null,"id":0,"e":"trade"}','{bad','{"e":"depth","tag":9}','{"data":{"e":"ticker","tag":10}}','{"e":"trade","tag":11}','{"e":"bookTicker","tag":12}','{"stream":"routeλ🚀","data":{"e":"kline","tag":13}}','{"data":0,"e":"trade","tag":14}','{"data":{"e":""}}','{"e":"unknown"}','{"error":{"code":4002}}','{"error":{"code":"4002"}}','{"error":{"code":7,"msg":"bad"}}','{"stream":"account.orderUpdate","data":{"tag":20}}','{"stream":"account.orderUpdate.BTC","data":{"tag":21}}','{"stream":"account.positionUpdate","data":{"tag":22}}','{"stream":17,"data":{"tag":23}}'];
const routing=[];
const specs=[...wires.map((_,input)=>({input,mode:'none',disposed:false})),...[[9,'track'],[9,'parse'],[9,'dispatch'],[19,'stringify'],[19,'emitLookup'],[19,'emit'],[9,'dispatchFormat'],[9,'dispatchEmit'],[20,'captureDispatch'],[22,'captureDispatch']].map(([input,mode])=>({input,mode,disposed:false})),{input:9,mode:'none',disposed:true},{input:19,mode:'emit',disposed:true}];
for(const private of [false,true])for(const spec of specs){
 const c=context(spec.mode),sock=new (private?Private:Public)(c.parent),dispatch=[],pongs=[];sock.isDisposed=spec.disposed;parseContext=c;c.socket=sock;c.dispatchName=spec.input===22?'handlePositionUpdate':'handleOrderUpdate';
 Object.defineProperty(sock,'isDisposed',{get(){c.calls.push('disposed');return spec.disposed;}});
 sock.trackLastReceived=()=>{c.calls.push('track');if(spec.mode==='track')throw c.errors.primary;};
 sock.handlePongEvent=interval=>{c.calls.push('pong');pongs.push(bits(interval));};
 for(const [method,kind] of [['handleDepthUpdate','depth'],['handleTickerUpdate','ticker'],['handleTradeUpdate','trade'],['handleBookTickerUpdate','bookTicker'],['handleKlineUpdate','kline'],['handleOrderUpdate','order'],['handlePositionUpdate','position']]){
  const callback=function(data,stream){c.calls.push('dispatch:'+kind);assert.equal(data,spec.mode==='captureDispatch'?c.payload:c.parsed.data||c.parsed);dispatch.push({kind,data,sameData:true,stream:kind==='kline'?(stream===undefined?null:typeof stream==='string'?units(stream):stream):null});if(['dispatch','dispatchFormat','dispatchEmit'].includes(spec.mode))throw c.errors.primary;};
  if(spec.mode==='captureDispatch'&&method===c.dispatchName)Object.defineProperty(sock,method,{configurable:true,get(){c.calls.push('method:'+kind);c.payload={tag:99};return callback;}});else sock[method]=callback;
 }

 let result;
 if(spec.mode==='stringify'){
  const original=parsing.jsonParse,value=original(wires[spec.input]);
  value.error.toJSON=()=>{c.calls.push('stringify');throw c.errors.primary;};
  parsing.jsonParse=()=>value;
  try{result=outcome(c,()=>sock.onMessage({data:wires[spec.input]}));}finally{parsing.jsonParse=original;}
 }else result=outcome(c,()=>sock.onMessage({data:wires[spec.input]}));
 routing.push({...spec,private,wire:units(wires[spec.input]),calls:c.calls,messages:c.messages,dispatch,pongs,result});
}
const samples=[['1x','0.25','3.5','7.75'],['bad','NaN','oops',''],['-0','Infinity','0','-0'],['1\ud800','\ud8002','\ufeff3.5','4x'],['1e309','-1e309','2.5e1junk','0x10'],['.25','-0','1e-400','-1e-400']];
const numeric=[];
for(const method of ['depth','ticker','trade','bookTicker']){
 const cases=[...samples.flatMap((_,input)=>['none','missing','update'].map(mode=>({input,mode}))),...['lookup','field','emitLookup','format','emit'].map(mode=>({input:0,mode})),...(method==='depth'?samples.flatMap((_,input)=>['emptyB','emptyA','emptyBoth'].map(mode=>({input,mode}))):[])];
 for(const spec of cases){
  const c=context(spec.mode),factory=[];throttleRecord=factory;const sock=new Public(c.parent);throttleRecord=undefined;
  const values=samples[spec.input],symbol=spec.input===3?'X\ud800':'BTC',ticker={},data={},patches=[];Object.defineProperty(data,'s',{get(){c.calls.push('symbol');return symbol;}});
  const fields=method==='ticker'?{c:values[0],P:values[1],v:values[2],q:values[3]}:method==='trade'?{p:values[0]}:method==='bookTicker'?{b:values[0],a:values[1]}:{};
  for(const [name,value] of Object.entries(fields))Object.defineProperty(data,name,{get(){c.calls.push('field:'+name);if(spec.mode==='field')throw c.errors.primary;return value;}});
  if(method==='depth')for(const [name,value] of [['b',values[0]],['a',values[1]]]){const row=[];Object.defineProperty(row,0,{get(){c.calls.push('price:'+name);return value;}});const levels=(spec.mode==='emptyBoth'||(name==='b'&&spec.mode==='emptyB')||(name==='a'&&spec.mode==='emptyA'))?[]:[row];Object.defineProperty(data,name,{get(){c.calls.push(name);if(spec.mode==='field')throw c.errors.primary;return levels;}});}
  c.parent.store={tickersBySymbol:new Proxy({}, {get(_t,key){c.calls.push('lookup');assert.equal(key,symbol);if(spec.mode==='lookup')throw c.errors.primary;return spec.mode==='missing'?undefined:ticker;}}),updateTicker(actual,patch){c.calls.push('update');assert.equal(actual,ticker);patches.push({sameTicker:true,fields:Object.fromEntries(Object.entries(patch).map(([k,v])=>[k,bits(v)]))});if(['update','emitLookup','format','emit'].includes(spec.mode))throw c.errors.primary;}};
  const methodName={depth:'updateTickerFromDepth',ticker:'handleTickerUpdate',trade:'handleTradeUpdate',bookTicker:'handleBookTickerUpdate'}[method];
  const result=outcome(c,()=>sock[methodName](data));numeric.push({...spec,method,symbol:units(symbol),inputValues:values.map(units),factory,calls:c.calls,messages:c.messages,patches,result});
 }
}
const delivery=[];
const modes=['none','absent','empty','stats','firstThrow','secondThrow','remove','append','replace','mutateSymbol','emitLookup','format','emit','lookup','throttle'];
for(const kind of ['depth','kline'])for(const mode of [...modes,...(kind==='kline'?['nullOpen','nullClose','emptyStream','loneStream','missingInterval']:['captureThrottle'])]){
 const c=context(mode),sock=new Public(c.parent),data={},called=[],topics=[],throttles=[];let symbol='λ\ud800';const interval='1m';const stream=kind==='kline'?(mode==='emptyStream'?'':mode==='loneStream'?'custom.\udc00':mode==='missingInterval'?undefined:'custom.stream'):undefined;
 Object.defineProperty(data,'s',{get(){c.calls.push('symbol');return symbol;}});Object.defineProperty(data,'i',{get(){c.calls.push('interval');return mode==='missingInterval'?undefined:interval;}});
 for(const field of ['o','c'])Object.defineProperty(data,field,{get(){c.calls.push(field);return (mode==='nullOpen'&&field==='o')||(mode==='nullClose'&&field==='c')?null:'1';}});
 let handlers;const h2=actual=>{assert.equal(actual,data);c.calls.push('handler:2');called.push(2);};
 const h0=actual=>{assert.equal(actual,data);c.calls.push('handler:0');called.push(0);if(mode==='remove')delete handlers[1];if(mode==='append')handlers.push(h2);if(mode==='replace')sock.messageHandlers={};if(mode==='mutateSymbol')symbol='after\udc00';if(['firstThrow','emitLookup','format','emit'].includes(mode))throw c.errors.primary;};
 const h1=actual=>{assert.equal(actual,data);c.calls.push('handler:1');called.push(1);if(mode==='secondThrow')throw c.errors.primary;};handlers=mode==='empty'?[]:[h0,h1];
 sock.messageHandlers=new Proxy({}, {get(_t,topic){c.calls.push('lookup');topics.push(units(topic));if(mode==='lookup')throw c.errors.primary;return mode==='absent'?undefined:handlers;}});
 sock.recordMessageHandlerStats=topic=>{c.calls.push('stats');assert.deepEqual(units(topic),topics[0]);if(mode==='stats')throw c.errors.primary;};
 sock.getDepthTickerUpdateThrottle=market=>{c.calls.push('throttle');throttles.push(units(market));return actual=>{assert.equal(actual,data);c.calls.push('invoke');if(mode==='throttle')throw c.errors.primary;};};
 if(mode==='captureThrottle'){const factory=sock.getDepthTickerUpdateThrottle;Object.defineProperty(sock,'getDepthTickerUpdateThrottle',{get(){c.calls.push('factory');symbol='factory\ud800';return factory;}});}
 const result=outcome(c,()=>kind==='depth'?sock.handleDepthUpdate(data):sock.handleKlineUpdate(data,stream));
 delivery.push({kind,mode,stream:stream===undefined?null:units(stream),symbol:units('λ\ud800'),interval:units(interval),calls:c.calls,messages:c.messages,called,topics,throttles,result});
}
process.stdout.write(JSON.stringify({routing,numeric,delivery}));
