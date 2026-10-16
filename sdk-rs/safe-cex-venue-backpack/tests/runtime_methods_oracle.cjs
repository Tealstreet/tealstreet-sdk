const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require},{filename:file});return exports;}
const severity=run(path.join(root,'packages/logging/src/LogSeverity.ts'),id=>unused(id));
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>id==='@tealstreet/logging/src/LogSeverity'?severity:unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
let xhrSetups=0,sockets=0;
class Base {createXhrWithSecondaryKey(){xhrSetups++;return unused('xhr');}}
class Socket {constructor(){sockets++;}}
const api=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base')return {BaseExchange:Base};
 if(id==='./backpack.api')return {createAPI:unused('createAPI')};
 if(id==='./backpack.ws-public')return {BackpackPublicWebSocket:Socket};
 if(id==='./backpack.ws-private')return {BackpackPrivateWebSocket:Socket};
 return unused(id);
});
const exchange=new api.BackpackExchange('backpack',{},{});
assert.equal(xhrSetups,1);assert.equal(sockets,2);

const bits=n=>{assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function observe(v){if(v===undefined)return ['undefined'];if(v===null)return ['null'];if(typeof v==='number')return ['number',bits(v)];if(typeof v==='bigint')return ['bigint',String(v)];if(typeof v==='string'||typeof v==='boolean')return [typeof v,v];if(Array.isArray(v))return ['array',v.map(observe)];return ['object',Object.keys(v).map(k=>[k,observe(v[k])])];}

const requests=()=>[{symbol:'BTC',side:'Bid',orderType:'Limit',clientId:101},{symbol:'ETH',side:'Ask',orderType:'Market',clientId:202}];
const rows=()=>[{operation:'Ok',clientId:101,symbol:'BTC',side:'Bid',orderType:'Limit',id:'order-a'},{operation:'Ok',clientId:202,symbol:'ETH',side:'Ask',orderType:'Market',id:'order-b'}];
const decode=[];
function add(label,data,req=requests()){
 if(arguments.length<2)data=rows();
 let outcome;try{const value=exchange.decodeBatchOrderResponse(data,req);assert.ok(Array.isArray(value));for(const id of value)assert.equal(typeof id,'string');outcome={ok:true,ids:value};}catch(e){outcome={ok:false,name:e.name,message:e.message};}
 decode.push({label,input:observe(data),requests:req.map(r=>({symbol:r.symbol,side:r.side,orderType:r.orderType,clientId:observe(r.clientId)})),outcome});
}
function field(label,key,value){const data=rows();if(value===undefined)delete data[0][key];else data[0][key]=value;add(label,data);}
add('forward');add('reversed',rows().reverse());add('empty',[],[]);
add('null data',null);add('undefined data',undefined);add('object data',{});add('short',rows().slice(0,1));add('long',[...rows(),rows()[0]]);
for(const [label,id] of [['request missing',undefined],['request zero',0],['request fraction',1.5],['request nan',NaN],['request infinity',Infinity],['request overflow',0x100000000]]){const req=requests();if(id===undefined)delete req[0].clientId;else req[0].clientId=id;add(label,rows(),req);}
{const req=requests();req[1].clientId=101;add('request duplicate',rows(),req);}
for(const [label,row] of [['row null',null],['row array',[]],['row primitive',1]])add(label,[row,rows()[1]]);
for(const [label,value] of [['operation missing',undefined],['operation null',null],['operation zero',0],['operation nan',NaN],['operation bigint',5n],['operation object',{}],['operation toString',{toString:0}]])field(label,'operation',value);
for(const [label,id] of [['client missing',undefined],['client string','101'],['client zero',0],['client negative',-1],['client nan',NaN],['client fraction',1.5],['client unknown',303]])field(label,'clientId',id);
{const data=rows();data[1].clientId=101;add('client duplicate',data);}
field('symbol mismatch','symbol','ETH');field('side mismatch','side','Ask');field('type mismatch','orderType','Market');
field('id missing','id',undefined);field('id number','id',42);field('id blank','id',' \t\uFEFF');
{const data=rows();data[1].id=' order-a ';add('id duplicate',data);}
{const data=rows();data[0].id=' \uFEFForder-a\t ';data[1].id='\norder-b\r';add('trimmed',data);}
{const data=rows(),req=requests();data[0].clientId=req[0].clientId=0xffffffff;add('uint32 max',data,req);}
{const data=rows(),req=requests();data[0].symbol=req[0].symbol='λ🚀';data[0].id='order-λ🚀';add('unicode',data,req);}
{const data=rows();data[0].operation=0;data[0].clientId=NaN;add('operation precedence',data);}
{const req=requests();delete req[0].clientId;add('count precedence',[],req);}
{const data=rows();data[0].clientId=303;data[1]=null;add('row precedence',data);}
field('NEL retained','id','\u0085order-a\u0085');
const req={symbol:'BTC\ud800',side:'Bid',orderType:'Limit',clientId:101};
const boundary={requestRaw:JSON.stringify(req),result:exchange.decodeBatchOrderResponse([{...req,operation:'Ok',id:'ok'}],[req])};
async function main(){
 const lifecycle=[];
 for(const initially of [false,true])for(const fail of ['none','public','private']){
  const trace=[],failure=Object.assign(new Error(fail+' dispose failed'),{name:'DisposeError'});exchange.isDisposed=initially;
  exchange.publicWebsocket={dispose(){trace.push(['public',exchange.isDisposed]);if(fail==='public')throw failure;}};
  exchange.privateWebsocket={dispose(){trace.push(['private',exchange.isDisposed]);if(fail==='private')throw failure;}};
  let outcome;try{assert.equal(exchange.dispose(),undefined);outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure};}
  lifecycle.push({initially,fail,trace,after:exchange.isDisposed,outcome});
 }
 const leverage=[];
 const stages=['ok','patch sync','patch async','info','positions sync','positions async','update','emit','error log'];
 for(const value of [2.5,-0,NaN,Infinity,-Infinity])for(const stage of stages){
  const trace=[],failure=Object.assign(new Error(stage+' failed'),{name:'StageError'}),replacement=Object.assign(new Error(stage+' replacement'),{name:'CallbackError'});
  const positions={opaque:'positions',value};exchange.accountLeverageLimit=17;
  exchange.xhr={patch(endpoint,data){trace.push(['patch',endpoint,data.leverageLimit]);if(stage==='patch sync')throw failure;return stage==='patch async'||stage==='emit'||stage==='error log'?Promise.reject(failure):Promise.resolve();}};
  exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(stage==='info'&&severity==='info')throw failure;if(stage==='error log'&&severity==='error')throw replacement;};
  exchange.emitter={emit(event,error){trace.push(['emit',event,error.name,error.message]);if(stage==='emit')throw replacement;}};
  exchange.fetchPositions=()=>{trace.push(['positions']);if(stage==='positions sync')throw failure;return stage==='positions async'?Promise.reject(failure):Promise.resolve(positions);};
  exchange.store={update({positions:actual}){assert.equal(actual,positions);trace.push(['update',observe(actual)]);if(stage==='update')throw failure;}};
  const pending=exchange.setLeverage('ignored',value);const atConstruction=structuredClone(trace),before=bits(exchange.accountLeverageLimit);
  let outcome;try{assert.equal(await pending,undefined);outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure,sameReplacement:e===replacement};}
  leverage.push({value:bits(value),stage,atConstruction,before,after:bits(exchange.accountLeverageLimit),trace,outcome});
 }
 const cancel=[];
 for(const stage of ['missing','ok','lookup','delete sync','delete async','logger']){
  const trace=[],failure=Object.assign(new Error(stage+' failed'),{name:'StageError'}),replacement=Object.assign(new Error('logger replacement'),{name:'CallbackError'});
  const symbols=stage==='lookup'?new Proxy({}, {get(){throw failure;}}):stage==='missing'?{}:{'λ🚀':{id:'VENUE-λ'}};
  exchange.store={marketsBySymbol:symbols};exchange.xhr={delete(endpoint,body){trace.push(['delete',endpoint,body.data.symbol]);if(stage==='delete sync')throw failure;return stage==='delete async'||stage==='logger'?Promise.reject(failure):Promise.resolve();}};
  exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(stage==='logger')throw replacement;};
  const pending=exchange.cancelSymbolOrders('λ🚀'),atConstruction=structuredClone(trace);
  let outcome;try{assert.equal(await pending,undefined);outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure,sameReplacement:e===replacement};}
  cancel.push({stage,atConstruction,trace,outcome});
 }
 exchange.store={marketsBySymbol:{['BTC\ud800']:{id:'venue'}}};exchange.xhr={delete(){return Promise.resolve();}};exchange.log=()=>assert.fail('unexpected boundary log');
 await exchange.cancelSymbolOrders('BTC\ud800');
 const cancelBoundary={symbolRaw:JSON.stringify('BTC\ud800'),ok:true};
 const opaque={},opaqueTrace=[];exchange.isDisposed=false;
 exchange.publicWebsocket={dispose(){opaqueTrace.push('public');throw opaque;}};exchange.privateWebsocket={dispose(){assert.fail('private after public failure');}};
 let sameOpaque=false;try{exchange.dispose();assert.fail('opaque throw');}catch(e){sameOpaque=e===opaque;}
 const lifecycleOpaque={after:exchange.isDisposed,trace:opaqueTrace,sameOpaque};
 const hostTrace=[],hostError={toString(){return 'opaque λ';}};exchange.accountLeverageLimit=17;
 exchange.xhr={patch(){hostTrace.push('patch');throw hostError;}};
 exchange.emitter={emit(event,error){hostTrace.push(['emit',event,error.name,error.message]);}};
 exchange.log=(message,severity)=>hostTrace.push(['log',message,severity]);
 let sameHost=false;try{await exchange.setLeverage('ignored',2.5);assert.fail('host throw');}catch(e){sameHost=e===hostError;}
 const leverageHost={after:bits(exchange.accountLeverageLimit),trace:hostTrace,sameHost};
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},decode,boundary,lifecycle,lifecycleOpaque,leverage,leverageHost,cancel,cancelBoundary}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
