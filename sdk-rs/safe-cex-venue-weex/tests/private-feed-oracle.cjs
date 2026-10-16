const assert = require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,load,capture}=require('./types-oracle.cjs');
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const venue=load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const parser=load(path.join(root,'packages/json-parse/src/jsonParse.ts'));
const names=['connectAndSubscribe','onMessage','dispose','handleOrders','handlePositions','handleFills','throttledFetchOrders','throttledFetchPositions','throttledFetchBalance'];
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.ws-private.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='WeexPrivateWebsocket');
const fields=cls.members.filter(n=>ts.isPropertyDeclaration(n)&&names.includes(n.name.getText(tree)));
assert.equal(fields.length,9,'private_feed::source_fields');
const utilsFile=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const utilTree=ts.createSourceFile(utilsFile,fs.readFileSync(utilsFile,'utf8'),ts.ScriptTarget.Latest,true);
const utilNames=['toNumber','normalizeTimestampToMs','normalizeTimestampToSeconds'];
const utilFields=utilTree.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>utilNames.includes(d.name.getText(utilTree))));
assert.equal(utilFields.length,3,'private_feed::source_utilities');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const text=u=>String.fromCharCode(...u);const bits=n=>capture(n).bits;
function encode(v){if(v===undefined)return {kind:'undefined'};if(v===null)return {kind:'null'};
 if(typeof v==='string')return{kind:'string',units:units(v)};
 if(typeof v==='number')return{kind:'number',bits:bits(v)};
 if(typeof v==='boolean')return{kind:'boolean',value:v};
 if(Array.isArray(v))return{kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?encode(v[i]):{kind:'hole'})};
 return{kind:'object',fields:Object.entries(v).map(([k,x])=>[k,encode(x)])};}
const cases=[];function add(id,method,patch={}){cases.push({id,input:{method,disposed:false,supported:true,credentials:{key:'K',secret:'S',passphrase:'P',testnet:false},json:{},data:null,subscriptionId:bits(1),stored:[],fail:'',headerError:false,callHeaders:false,timerKind:'orders',fire:false,disposeDuringFetch:false,oldTimers:[11,12,13],afterAddClosed:false,appendFill:false,holes:[],customIterator:false,...patch}});}
add('connect_basic','connect',{callHeaders:true});add('connect_disposed','connect',{disposed:true});add('connect_browser','connect',{supported:false});
add('connect_key_missing','connect',{credentials:{key:'',secret:'S',passphrase:'P'}});add('connect_secret_missing','connect',{credentials:{key:'K',secret:'',passphrase:'P'}});
add('connect_pass_missing','connect',{credentials:{key:'K',secret:'S',passphrase:''}});add('connect_testnet','connect',{credentials:{key:'K',secret:'S',passphrase:'P',testnet:true}});
add('connect_header_failure','connect',{fail:'create',headerError:true});add('connect_other_failure','connect',{fail:'create'});add('connect_listener_failure','connect',{fail:'listener:message'});
add('message_disposed','message',{disposed:true,data:'bad'});add('message_invalid','message',{data:'bad'});add('message_null','message',{data:'null'});
add('message_ping','message',{data:'{"type":"ping","e":"orders"}'});add('message_ping_send_failure','message',{data:'{"type":"ping"}',fail:'sendPong'});
add('message_result_false','message',{data:'{"result":false,"msg":"DENIED","e":"orders"}'});add('message_result_default','message',{data:'{"result":false,"msg":0}'});
add('message_result_null','message',{data:'{"result":null,"e":"account"}'});add('message_missing_event','message',{data:'{}'});add('message_unknown','message',{data:'{"e":"unknown"}'});
add('message_account','message',{data:'{"e":"account"}'});add('message_track_disposes','message',{data:'{"type":"ping"}',fail:'',disposeOnTrack:true});
add('message_large_fill','message',{data:'{"e":"fill","E":1700000000000,"d":[{"id":1234567890123456789,"orderId":9,"orderSide":"SELL","fillValue":"12","fillSize":"3","symbol":"BTC"}]}'});
const row=(id,status='open')=>({symbol:'BTC',normalized:{id,status}});
add('orders_closed','orders',{json:{d:[row('OPEN'),row('CLOSED','closed'),row('CANCEL','canceled')]}});
add('orders_null_normalizer','orders',{json:{d:[{symbol:'BTC',normalized:null},row('YES')]}});
add('orders_non_array','orders',{json:{d:{}}});add('orders_add_changes_status','orders',{json:{d:[row('CHANGE')]},afterAddClosed:true});
add('orders_add_failure','orders',{json:{d:[row('NO')]},fail:'addOrders'});add('orders_update_failure','orders',{json:{d:[row('NO','closed')]},fail:'updateStatus'});
add('orders_sparse','orders',{json:{d:[row('A'),row('HOLE'),row('B')]},holes:[1]});
add('positions_basic','positions',{json:{d:[{symbol:'BTC',normalized:{id:'LONG'}},{symbol:'BTC',normalized:null}]}});
add('positions_non_array','positions',{json:{d:null}});add('positions_add_failure','positions',{json:{d:[{symbol:'BTC',normalized:{id:'P'}}]},fail:'addPositions'});
const fill={id:'F',orderId:'O',orderSide:'SELL',symbol:'BTC',fillValue:'12',fillSize:'-3',fillFee:'-.2',realizePnl:'-4',createdTime:1700000000000};
add('fills_basic','fills',{json:{E:99,d:[fill]}});add('fills_order_side','fills',{json:{d:[fill]},stored:[{id:'O',side:'buy'}]});
add('fills_zero_amount','fills',{json:{d:[{...fill,fillSize:0}]}});add('fills_zero_value','fills',{json:{d:[{...fill,fillValue:0}]}});
add('fills_fallback_fields','fills',{json:{E:1700000000000,d:[{...fill,id:'',orderId:0,createdTime:0}]}});
add('fills_unicode_side','fills',{json:{d:[{...fill,orderSide:'ſell'}]}});add('fills_infinity','fills',{json:{d:[{...fill,fillValue:'1e308',fillSize:'1e-308'}]}});
add('fills_live_append','fills',{json:{d:[fill]},appendFill:true});add('fills_sparse','fills',{json:{d:[fill,fill]},holes:[0]});
add('fills_emit_failure_close','fills',{json:{d:[fill]},fail:'emitFill',customIterator:true});add('fills_non_array','fills',{json:{d:false}});
for(const kind of ['orders','positions','balance']){add(kind+'_schedule','timer',{timerKind:kind});add(kind+'_refresh','timer',{timerKind:kind,fire:true});add(kind+'_disposed_after_fetch','timer',{timerKind:kind,fire:true,disposeDuringFetch:true});add(kind+'_fetch_failure','timer',{timerKind:kind,fire:true,fail:'fetch:'+kind});add(kind+'_apply_failure','timer',{timerKind:kind,fire:true,fail:'apply:'+kind});}
add('orders_missing_timer','timer',{oldTimers:[]});add('dispose_missing_timers','dispose',{oldTimers:[]});add('dispose_basic','dispose');add('dispose_clear_failure','dispose',{fail:'clear:12'});
async function run(c){const input=c.input,events=[],created=[],listeners=[],headers=[],warnings=[],addedOrders=[],statuses=[],addedPositions=[],fills=[],timers=[],applied=[],rawPongs=[];let disposed=input.disposed,id=Buffer.from(input.subscriptionId,'hex').readDoubleBE(),nonce=1,provider,ws,callback,handle=100;
 const timersByKind={orders:input.oldTimers[0],positions:input.oldTimers[1],balance:input.oldTimers[2]};
 function check(stage){if(input.fail===stage)throw new RangeError('fail:'+stage);}
 class HeaderCapableWebSocketUnavailableError extends Error{}
 class Base{dispose(){events.push('baseDispose');check('baseDispose');disposed=true;}}
 function setTimeout(cb,delay){events.push('schedule:'+input.activeKind);check('schedule');timers.push({kind:input.activeKind,delay,id:handle});callback=cb;return handle++;}
 function clearTimeout(timer){events.push('clear:'+timer);check('clear:'+timer);}
 const compiled=ts.transpileModule(utilFields.map(n=>n.getText(utilTree)).join('\n')+'\nexport class Slice extends Base {'+fields.map(n=>n.getText(tree)).join('\n')+'}',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 const scope={exports:{},JSON:{stringify(value,...args){if(value?.method==='PONG')rawPongs.push(bits(value.id));return JSON.stringify(value,...args);}},...enums,...venue,Base,HeaderCapableWebSocketUnavailableError,setTimeout,clearTimeout,
 isNodeOrBunRuntime(){events.push('supported');return input.supported;},bigIntAsStringJsonParse(s){events.push('parse');check('parse');return parser.bigIntAsStringJsonParse(s);},
 normalizeOrder(row,market){events.push('normalizeOrder');check('normalizeOrder');return row.normalized;},normalizePosition(row,market){events.push('normalizePosition');check('normalizePosition');return row.normalized;}};
 vm.runInNewContext(compiled.outputText,scope);const instance=new scope.exports.Slice();
 Object.defineProperty(instance,'isDisposed',{get(){events.push('disposed');check('disposed');return disposed;}});
 Object.defineProperty(instance,'subscriptionId',{get(){events.push('id:get');return id;},set(v){events.push('id:set');id=v;}});
 for(const kind of ['orders','positions','balance'])Object.defineProperty(instance,'refresh'+kind[0].toUpperCase()+kind.slice(1)+'Timeout',{get(){events.push('timer:'+kind);return timersByKind[kind];},set(v){events.push('assign:'+kind);timersByKind[kind]=v;}});
 instance.logUnsupportedTransport=reason=>{events.push('unsupported');warnings.push(units(reason??'default'));};
 instance.createAuthHeaders=()=>{events.push('auth');check('auth');const result={nonce};headers.push(encode(result));return result;};
 instance.createWebsocket=(url,protocols,options)=>{events.push('create');created.push({url:units(url),protocols:protocols??null,headersRequired:options.headersRequired});provider=options.headers;if(input.headerError)throw new HeaderCapableWebSocketUnavailableError('header');check('create');return{addEventListener(event,fn){events.push('listener:'+event);check('listener:'+event);listeners.push(event);}};};
 Object.defineProperty(instance,'ws',{get(){events.push('socket');return ws;},set(v){events.push('socket:set');check('socket:set');ws=v;}});
 instance.onOpen=()=>{};instance.onClose=()=>{};
 instance.trackLastReceived=()=>{events.push('track');check('track');if(input.disposeOnTrack)disposed=true;};
 const pongs=[];instance.send=s=>{events.push('sendPong');check('sendPong');pongs.push(JSON.parse(s));};instance.handlePongEvent=interval=>{events.push('pong');applied.push({kind:'pong',value:encode(interval)});};
 instance.recordMessageHandlerStats=event=>{events.push('stats');applied.push({kind:'stats',value:encode(event)});};
 const market={};const store={orders:input.stored,addOrUpdateOrders(orders){events.push('addOrders');check('addOrders');if(input.afterAddClosed)orders[0].status='closed';addedOrders.push(encode(orders));},updateOrderStatus({id},status){events.push('updateStatus');check('updateStatus');statuses.push({id:encode(id),status:encode(status)});},addOrUpdatePositions(p){events.push('addPositions');check('addPositions');addedPositions.push(encode(p));},addOrUpdatePositionsFull(p){events.push('apply:positions');check('apply:positions');applied.push({kind:'positions',value:encode(p)});},update(v){const kind=Object.keys(v)[0];events.push('apply:'+kind);check('apply:'+kind);applied.push({kind,value:encode(v[kind])});}};
 Object.defineProperty(store,'marketsBySymbol',{get(){events.push('markets');return new Proxy({},{get(_,key){events.push('market');return market;}});}});
 instance.store=store;
 const parent={options:new Proxy(input.credentials,{get(target,key){events.push('credential:'+key);check('credential:'+key);return target[key];}}),log(message,severity){assert.equal(severity,enums.LogSeverity.Warning);events.push('warning');check('warning');warnings.push(units(message));},emitter:{emit(event,value){assert.equal(event,'fill');events.push('emitFill');check('emitFill');fills.push({id:units(value.id),orderId:units(value.orderId),side:encode(value.side),symbol:encode(value.symbol),price:bits(value.price),amount:bits(value.amount),timestamp:bits(value.timestamp),fee:bits(value.fee),realizedPnl:bits(value.realizedPnl)});if(input.appendFill&&fills.length===1)input.json.d.push({...fill,id:'APPENDED'});}}};
 for(const kind of ['orders','positions','balance'])parent['fetch'+kind[0].toUpperCase()+kind.slice(1)]=async()=>{events.push('fetch:'+kind);check('fetch:'+kind);await Promise.resolve();events.push('resolved:'+kind);if(input.disposeDuringFetch)disposed=true;return{refreshed:kind};};instance.parent=parent;
 for(const kind of ['orders','positions','balance']){const name='throttledFetch'+kind[0].toUpperCase()+kind.slice(1),original=instance[name];instance[name]=()=>{input.activeKind=kind;return original();};}
 for(const index of input.holes)delete input.json.d[index];
 if(input.customIterator)Object.defineProperty(input.json.d,Symbol.iterator,{value(){let index=0;return{next(){return index<input.json.d.length?{value:input.json.d[index++],done:false}:{done:true};},return(){events.push('iteratorClose');return{done:true};}};}});
 let error=null,callbackError=null;try{if(input.method==='connect'){instance.connectAndSubscribe();if(input.callHeaders&&provider){nonce=2;provider();}}else if(input.method==='message')instance.onMessage({data:input.data});else if(input.method==='orders')instance.handleOrders(input.json);else if(input.method==='positions')instance.handlePositions(input.json);else if(input.method==='fills')instance.handleFills(input.json);else if(input.method==='dispose')instance.dispose();else{instance['throttledFetch'+input.timerKind[0].toUpperCase()+input.timerKind.slice(1)]();if(input.fire){const promise=callback();events.push('callbackStarted');await promise;}}}catch(err){error={name:err.name,units:units(err.message)};}
 return{id:c.id,input:{...c.input,json:undefined},jsonInput:encode(input.json),error,callbackError,disposed,subscriptionId:bits(id),created,listeners,headers,warnings,addedOrders,statuses,addedPositions,fills,timers,applied,rawPongs,events};}
(async()=>{const output=[];for(const c of cases){const original=structuredClone(c.input);const result=await run(c);result.input=original;output.push(result);}assert.equal(output.length,63);process.stdout.write(JSON.stringify(output));})().catch(err=>{console.error(err);process.exitCode=1;});
