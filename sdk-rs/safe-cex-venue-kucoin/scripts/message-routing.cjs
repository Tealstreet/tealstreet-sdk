const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/kucoin');
function member(file,ownerName,name){const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===ownerName),m=owner.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,'maintained '+ownerName+'.'+name);return m.getText(ast);}
const owners={'futures-public':['kucoin.ws-public.ts','KucoinPublicWebsocket'],'futures-private':['kucoin.ws-private.ts','KucoinPrivateWebsocket'],'spot-public':['kucoinspot.ws-public.ts','KucoinSpotPublicWebsocket'],'spot-private':['kucoinspot.ws-private.ts','KucoinSpotPrivateWebsocket']};
const methods=Object.fromEntries(Object.entries(owners).map(([k,[f,o]])=>[k,member(path.join(root,f),o,'onMessage')]));
const parsePath=path.resolve(root,'../../../../json-parse/src/jsonParse.ts');
const parseAst=ts.createSourceFile(parsePath,fs.readFileSync(parsePath,'utf8'),ts.ScriptTarget.Latest,true),parseCode=parseAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(parseAst)==='jsonParse')).getText(parseAst);
const parser={exports:{}};vm.runInThisContext('(function(module,exports){'+ts.transpileModule(parseCode,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText+'\n})')(parser,parser.exports);
const typeFile=path.resolve(root,'../../types.ts'),typeAst=ts.createSourceFile(typeFile,fs.readFileSync(typeFile,'utf8'),ts.ScriptTarget.Latest,true),orderStatusSource=typeAst.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderStatus').getText(typeAst),enums={exports:{}};
vm.runInThisContext('(function(module,exports){'+ts.transpileModule(orderStatusSource,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText+'\n})')(enums,enums.exports);
const input=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(input.routes.length,128,'independent route floor');assert.equal(input.algo.length,14,'independent algo floor');
const text=units=>String.fromCharCode(...units),units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const output={routes:[],algo:[]};
for(const c of input.routes){
 const trace=[],error=new TypeError('routing failure');let original,receiver,reads=0,current=c.topic===null?undefined:text(c.topic);
 const raw=c.scenario===2?'invalid':c.scenario===22?'0':c.scenario===23?'false':JSON.stringify({type:c.type??undefined,topic:current,payload:17});
 const jsonParse=data=>{trace.push(['parse',null]);if(c.fail==='parse')throw error;const j=parser.exports.jsonParse(data);if(j&&typeof j==='object'){
  original=j;if(c.scenario===24)current={startsWith:p=>p==='/contractMarket/level2',split:()=>['prefix',7]};Object.defineProperty(j,'type',{get(){if(c.fail==='type')throw error;return c.type??undefined;}});
  Object.defineProperty(j,'topic',{get(){const value=current;if(c.scenario===20&&++reads===1)current='/contractMarket/snapshot:BTC';else if(c.scenario!==20)reads++;return value;}});
 }return j;};
 const module={exports:{}},code=ts.transpileModule('export class Selected {'+methods[c.kind]+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 vm.runInNewContext('(function(module,exports){'+code+'\n})',{jsonParse})(module,module.exports);receiver=new module.exports.Selected();receiver.isDisposed=c.disposed;receiver.pingInterval=5000;
 const effect=(name,j,key)=>{if(j){assert.equal(j,original,'source original parsed identity');assert.equal(j.payload,17,'source payload');}trace.push([name,key===undefined?null:units(key)]);if(c.fail===name||c.fail==='handler'&&j)throw error;if(c.flip&&name==='track')receiver.isDisposed=true;};
 receiver.trackLastReceived=()=>effect('track');receiver.emitWsDiagnosticReceive=data=>{assert.equal(data,raw);effect('diagnostic');};receiver.handlePongEvent=interval=>{if(c.kind.startsWith('spot'))assert.equal(interval,5000);effect(c.kind.startsWith('spot')?'pong-interval':'pong');};receiver.subscribe=()=>effect('subscribe');receiver.recordMessageHandlerStats=k=>effect('stats',undefined,k);
 for(const [method,name]of [['handleTickerV2','tickerV2'],['handleSnapshot','snapshot'],['handleInstrument','instrument'],['handleOrderTopic','order'],['handleAlgoOrdersTopic','algo'],['handleTickerSnapshot','ticker']])receiver[method]=j=>effect(name,j);
 receiver.refreshSnapshotSoon=()=>effect('refresh-snapshot');receiver.refreshBalanceSoon=()=>effect('refresh-balance');receiver.parent={toInternalSymbol:s=>{if(c.scenario===24)assert.equal(typeof s,'number','source opaque split component');trace.push(['internal:'+s,null]);return c.scenario===24?undefined:s?'INTERNAL':undefined;}};
 receiver.getCandleHandlerTopic=topic=>{assert.equal(topic,'/market/candles:BTC_1min');trace.push(['candle-helper',null]);return '\ud8003';};
 const arrays=new Map;receiver.messageHandlers=new Proxy({}, {get(_,key){if(!arrays.has(key)){let a;const cb=id=>j=>{assert.equal(j,original,'source fanout identity');assert.equal(j.payload,17);trace.push(['cb:'+id,null]);if(id===1)a.push(cb(3));};a=[cb(1),cb(2)];arrays.set(key,a);}
   const a=arrays.get(key);if(c.kind==='futures-public'){
    // The first table lookup is the truthiness guard; the second starts iteration.
    a.reads=(a.reads??0)+1;if(a.reads===2)effect('for-of',original,key);
   }else if(c.kind==='spot-public'){
    if(!Object.hasOwn(a,'forEach'))a.forEach=function(fn){effect('for-each',original,key);return Array.prototype.forEach.call(this,fn);};
   }return a;}});
 let failed=false;try{receiver.onMessage({data:raw});}catch(e){assert.equal(e,error,'source original error identity');failed=true;}
 output.routes.push({trace,error:failed,topicReads:reads});
}
const algo=member(path.join(root,'kucoin.ws-private.ts'),'KucoinPrivateWebsocket','handleAlgoOrdersTopic');
for(const scenario of input.algo){const trace=[],error=new TypeError('routing failure'),module={exports:{}},code=ts.transpileModule('export class Selected {'+algo+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;vm.runInNewContext('(function(module,exports){'+code+'\n})',{OrderStatus:enums.exports.OrderStatus})(module,module.exports);const receiver=new module.exports.Selected();
 const fail=({10:'event',11:'map',12:'first',13:'update'})[scenario]??'',call=name=>{trace.push([name,null]);if(name===fail)throw error;};
 let event=([1,7,9,10,13].includes(scenario)?'cancel':scenario===2?'triggered':scenario===3?'active':[4,8].includes(scenario)?'open':scenario===6?'':undefined),read=0;
 const raw={get type(){call('event');return event;},status:scenario===6?'cancel':undefined};const message={get data(){call(++read===1?'has':'data');return scenario===0?undefined:raw;}};const order={id:7};
 receiver.parent={mapOrder:r=>{assert.equal(r,raw);call('map');if(scenario===9)event='active';const a=[];Object.defineProperty(a,'0',{get(){call('first');return [5,10].includes(scenario)?undefined:order;}});return a;}};
 receiver.store={updateOrderStatus(p,status){assert.equal(p,order);assert.equal(p.id,7);assert.equal(status,enums.exports.OrderStatus.Canceled);assert.equal(status,'canceled','independent native status string');trace.push(['cancel',null]);if(fail==='update')throw error;},addOrUpdateOrder(p){assert.equal(p,order);assert.equal(p.id,7);trace.push(['add',null]);}};
 let failed=false;try{receiver.handleAlgoOrdersTopic(message);}catch(e){assert.equal(e,error);failed=true;}output.algo.push({trace,error:failed});
}
process.stdout.write(JSON.stringify(output));
