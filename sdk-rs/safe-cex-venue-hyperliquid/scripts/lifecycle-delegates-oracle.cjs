'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const sourceNames={core:['listenOHLCV','listenOrderBook','dispose','endRetryChain','scheduleOrderRefresh'],spot:['listenOHLCV','listenOrderBook','dispose'],public:['dispose'],private:['dispose']};
const names=['core.listenOHLCV','core.listenOrderBook','spot.listenOHLCV','spot.listenOrderBook','core.dispose','spot.dispose','public.dispose','private.dispose','core.endRetryChain','core.scheduleOrderRefresh'];
let g,ids,tags,nextId,calls,raw,registered;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const tag=(v,t)=>(tags.set(v,t),v);
function native(v){if(v===undefined)return ['undefined'];if(v===null)return ['null'];if(typeof v==='boolean')return ['bool',v];if(typeof v==='string')return ['string',units(v)];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return ['number',Number.isNaN(v)?'nan':b.toString('hex')];}if(!ids.has(v))ids.set(v,nextId++);return ['ref',ids.get(v),tags.get(v)||(v?.name?.endsWith('Error')?'error:'+v.name:typeof v)];}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(error){e.throw=native(error);throw error;}}
const globals={};
class Base{dispose(){raw.baseDisposed++;}}
const bind={
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish get');return Reflect.get(Object(object),name,object);}),
 __superGet:(receiver,name)=>op('superGet',{receiver:native(receiver),name},()=>{if(g.throwSuper)throw g.error;return Reflect.get(Base.prototype,name,receiver);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;return undefined;}),
 __call:(fn,receiver,...args)=>op('call',{function:native(fn),receiver:native(receiver),args:args.map(native)},()=>{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;return Reflect.apply(fn,receiver,args);}),
 __callback:(exchange,kind,fn)=>op('callback',{exchange:native(exchange),kind},()=>{tag(fn,kind+'Callback');return fn;}),
 __subtract:(left,right)=>op('subtract',{left:native(left),right:native(right)},()=>left-right),
 __greaterThan:(left,right)=>op('gt',{left:native(left),right:native(right)},()=>left>right),
 __template:value=>op('template',{value:native(value)},()=>`${value}`),
};
const globalNames=new Set(['setTimeout','clearTimeout','Date','LogSeverity']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args),variable=(name,value)=>f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(name,undefined,undefined,value)],ts.NodeFlags.Const));
 const visit=n=>{
  if(ts.isTypeNode(n))return n;
  if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&ts.isPropertyAccessExpression(n.left))return call('__assign',[ts.visitNode(n.left.expression,visit),f.createStringLiteral(n.left.name.text),ts.visitNode(n.right,visit)]);
  if(ts.isBinaryExpression(n)&&[ts.SyntaxKind.MinusToken,ts.SyntaxKind.GreaterThanToken].includes(n.operatorToken.kind))return call(n.operatorToken.kind===ts.SyntaxKind.MinusToken?'__subtract':'__greaterThan',[ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);
  if(ts.isTemplateExpression(n))return f.updateTemplateExpression(n,n.head,n.templateSpans.map(s=>f.updateTemplateSpan(s,call('__template',[ts.visitNode(s.expression,visit)]),s.literal)));
  if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&globalNames.has(n.expression.text))return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...n.arguments.map(a=>ts.isArrowFunction(a)?call('__callback',[f.createThis(),f.createStringLiteral('timeout'),ts.visitNode(a,visit)]):ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
   const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
   const superCall=n.expression.expression.kind===ts.SyntaxKind.SuperKeyword;
   const vars=[variable(recv,superCall?f.createThis():ts.visitNode(n.expression.expression,visit))];
   const nullish=name=>f.createIfStatement(f.createBinaryExpression(name,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createReturnStatement(f.createIdentifier('undefined')));
   if(n.expression.questionDotToken)vars.push(nullish(recv));
   vars.push(variable(fn,call(superCall?'__superGet':'__get',[recv,f.createStringLiteral(n.expression.name.text)])));
   if(n.questionDotToken)vars.push(nullish(fn));
   const args=n.arguments.map(a=>ts.isArrowFunction(a)?call('__callback',[f.createThis(),f.createStringLiteral('error'),ts.visitNode(a,visit)]):ts.visitNode(a,visit));
   return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...vars,f.createReturnStatement(call('__call',[fn,recv,...args]))],true))),undefined,[]);
  }
  if(ts.isPropertyAccessExpression(n))return call('__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
  if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
  return ts.visitEachChild(n,visit,context);
 };return s=>ts.visitNode(s,visit);
}
const classes={};
for(const [kind,methods] of Object.entries(sourceNames)){
 const stem=kind==='core'?'hyperliquid.exchange':kind==='spot'?'hyperliquidspot.exchange':kind==='public'?'hyperliquid.ws-public':'hyperliquid.ws-private';
 const file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.ts`),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
 const text=methods.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const transformed=ts.transform(m.initializer??m.body,[transformer]);const printed=printer.printNode(m.initializer?ts.EmitHint.Expression:ts.EmitHint.Unspecified,transformed.transformed[0],ast);transformed.dispose();return m.initializer?`${name}=${printed};`:`${name}(${m.parameters.map(p=>p.name.getText(ast)).join(',')})${printed}`;}).join('\n');
 const context={...bind,Base,exports:{}};vm.runInNewContext(ts.transpileModule(`class ${cls.name.text} extends Base{${text}}\nexports.Class=${cls.name.text};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);classes[kind]=context.exports.Class;
}
const stateKeys=['_fetchOrdersInFlight','_orderRefreshDebounceTimer','_hip3PositionRefreshDebounceTimer','_retryChainTimer','_retryChainActive','_orderRefreshPending','_lastOrderRefreshTime'];
const cases=[];
function add(kind,name,label,config={},args=[]){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];registered=[];
 g={error:tag({label},'error'),...config};
 const exchange=tag(new classes[kind](),'exchange');
 raw={_fetchOrdersInFlight:tag({},'inFlight'),_orderRefreshDebounceTimer:Object.hasOwn(config,'timer')?config.timer:111,_hip3PositionRefreshDebounceTimer:Object.hasOwn(config,'timer')?config.timer:222,_retryChainTimer:Object.hasOwn(config,'timer')?config.timer:333,_retryChainActive:true,_orderRefreshPending:config.pending,_lastOrderRefreshTime:Object.hasOwn(config,'last')?config.last:1000,baseDisposed:0,publicDisposed:0,privateDisposed:0,pingStopped:0,closed:0};
 const reads={};for(const key of stateKeys)Object.defineProperty(exchange,key,{get(){if(config.freshTimers&&key.endsWith('Timer'))return ++reads[key]===1?raw[key]:91.25;return raw[key];},set(value){if(config.rejectSet===key)throw tag(new TypeError('rejected setter'),'error:TypeError');raw[key]=value;}});
 if(config.freshTimers)for(const key of stateKeys)reads[key]=0;
 const tif=tag(new Map([['91','Gtx'],['92','Alo']]),'tif'),dexes=tag(new Set(['dex:1','dex:2']),'dexes'),idsSet=tag(new Set(['91','92']),'ids');
 tif.clear=tag(function(){Map.prototype.clear.call(this);},'tif.clear');for(const [s,t] of [[dexes,'dexes'],[idsSet,'ids']])s.clear=tag(function(){Set.prototype.clear.call(this);},t+'.clear');
 exchange._placedLimitOrderTif=tif;exchange._pendingHip3PositionRefreshDexes=dexes;exchange._pendingRetryOrderIds=idsSet;
 exchange.HIP3_ORDER_REFRESH_INTERVAL_MS=Object.hasOwn(config,'interval')?config.interval:60000;
 const cleanup=tag(()=>{},'cleanup');
 const makeSocket=t=>tag({dispose:tag(function(){raw[t+'Disposed']++;},t+'.dispose'),listenOHLCV:tag(function(){return cleanup;},'listenOHLCV'),listenOrderBook:tag(function(){return cleanup;},'listenOrderBook')},t);
 exchange.publicWebsocket=Object.hasOwn(config,'public')?config.public:makeSocket('public');exchange.privateWebsocket=Object.hasOwn(config,'private')?config.private:makeSocket('private');
 exchange.stopPingLoop=tag(function(){raw.pingStopped++;},'stopPingLoop');
 const ws=tag({close:tag(function(){raw.closed++;},'close')},'ws');if(Object.hasOwn(config,'close'))ws.close=config.close;
 exchange.ws=Object.hasOwn(config,'ws')?config.ws:ws;
 if(kind==='core')tag(exchange.endRetryChain,'endRetryChain');
 const promise=tag({catch:tag(function(callback){if(Object.hasOwn(config,'catchError'))callback(config.catchError);return this;},'catch')},'refreshPromise');
 exchange.refreshOrders=tag(function(){return promise;},'refreshOrders');exchange.log=tag(function(){},'log');
 globals.Date=tag({now:tag(function(){return Object.hasOwn(config,'now')?config.now:61001;},'Date.now')},'Date');globals.LogSeverity=tag({Warning:'warning'},'LogSeverity');
 globals.clearTimeout=tag(function(){},'clearTimeout');globals.setTimeout=tag(function(fn){registered.push(fn);return tag({},'timerId');},'setTimeout');tag(Base.prototype.dispose,'base.dispose');
 const input=native(exchange);args=args.map((v,i)=>typeof v==='object'&&v!==null?tag(v,i===1?'callback':'options'):v);
 function snapshot(){return {fields:stateKeys.map(k=>[k,native(raw[k])]),collections:[Array.from(tif.keys()),Array.from(dexes),Array.from(idsSet)],effects:[raw.baseDisposed,raw.publicDisposed,raw.privateDisposed,raw.pingStopped,raw.closed]};}
 const initial=snapshot(),inputArgs=args.map(native);let result;
 try{result={value:native(Reflect.apply(exchange[name],exchange,args))};}catch(error){result={throw:native(error)};}
 const callbacks=[];
 if(kind==='core'&&name==='scheduleOrderRefresh'&&config.fire!==false){for(const callback of registered){try{callbacks.push({value:native(bind.__call(callback,undefined))});}catch(error){callbacks.push({throw:native(error)});}}}
 cases.push({kind,name,label,input,args:inputArgs,calls,initial,result,callbacks,catchError:Object.hasOwn(config,'catchError')?{value:native(config.catchError)}:null,catchCallThrows:config.throwCall==='catch',endRetryCallThrows:config.throwCall==='endRetryChain',final:snapshot()});
}
for(const kind of ['core','spot'])for(const method of ['listenOHLCV','listenOrderBook']){
 const args=[method==='listenOHLCV'?{symbol:'BTC-PERP',interval:'1m'}:'BTC-PERP',{}];
 add(kind,method,'delegate',{},args);add(kind,method,'native argument alias',{},[91.25,{}]);
 for(const config of [{throwGet:'publicWebsocket'},{throwGet:method},{throwCall:method},{public:null}])add(kind,method,'delegate boundary',config,args);
}
add('core','dispose','three distinct timers');
for(const timer of [undefined,null,false,0,NaN,'',17.5,{}])add('core','dispose','timer truthiness',{timer});
for(const config of [{freshTimers:true},{public:null},{private:null},{throwSet:'_fetchOrdersInFlight'},{throwCall:'tif.clear'},{throwCall:'dexes.clear'},{throwCall:'clearTimeout'},{throwSet:'_orderRefreshDebounceTimer'},{throwCall:'endRetryChain'},{throwCall:'public.dispose'},{throwCall:'private.dispose'},{throwCall:'base.dispose'}])add('core','dispose','cleanup boundary',config);
for(const config of [{},{throwSuper:true},{throwCall:'base.dispose'},{throwCall:'public.dispose'},{throwCall:'private.dispose'},{public:null},{private:null}])add('spot','dispose','dispose ordering',config);
for(const config of [{},{throwCall:'stopPingLoop'},{throwSuper:true},{throwCall:'base.dispose'}])add('public','dispose','ping before base',config);
for(const ws of [undefined,null,false,0,'',{}, {close(){}}])add('private','dispose','optional socket',{ws});
for(const close of [undefined,null,false,0])add('private','dispose','optional close',{close});
for(const config of [{throwGet:'ws'},{throwGet:'close'},{throwCall:'close'},{throwCall:'stopPingLoop'}])add('private','dispose','close boundary',config);
for(const config of [{},{throwSet:'_retryChainActive'},{rejectSet:'_retryChainActive'},{throwGet:'_pendingRetryOrderIds'},{throwCall:'ids.clear'}])add('core','endRetryChain','retry cleanup',config);
for(const pending of [undefined,null,false,0,NaN,'',true,{}])add('core','scheduleOrderRefresh','pending gate',{pending});
for(const timer of [undefined,null,false,0,NaN,'',17.5,{}])add('core','scheduleOrderRefresh','prior timer',{timer});
for(const config of [{now:60999},{now:61000},{now:61001},{now:NaN},{now:Infinity},{last:'1000'}])add('core','scheduleOrderRefresh','refresh threshold',config);
for(const config of [{throwSet:'_orderRefreshPending'},{throwGet:'_orderRefreshDebounceTimer'},{throwCall:'clearTimeout'},{throwCall:'setTimeout'},{throwSet:'_orderRefreshDebounceTimer'},{throwGlobal:'Date'},{throwCall:'Date.now'},{throwGet:'_lastOrderRefreshTime'},{throwGet:'HIP3_ORDER_REFRESH_INTERVAL_MS'},{throwCall:'refreshOrders'},{throwCall:'catch'},{catchError:'rejected💥'},{catchError:Symbol('rejection')}])add('core','scheduleOrderRefresh','callback boundary',config);
assert.equal(cases.length,111,'independent24+21+7+4+15+5+35');
const baseline=cases.find(c=>c.kind==='core'&&c.name==='dispose'&&c.label==='three distinct timers');
assert.deepEqual(baseline.final.collections,[[],[],[]],'DISPOSE_BASELINE clears three native collections');
assert.deepEqual(baseline.final.effects,[1,1,1,0,0],'DISPOSE_BASELINE base/public/private exactly once');
const timer=cases.find(c=>c.name==='scheduleOrderRefresh'&&c.label==='refresh threshold'&&c.calls.some(e=>e.op==='gt'&&e.result?.[1]===true));
assert(timer&&timer.calls.some(e=>e.op==='get'&&e.args.name==='refreshOrders'),'TIMER_BASELINE actual refresh call');
assert(cases.some(c=>c.name==='scheduleOrderRefresh'&&c.calls.some(e=>e.op==='template'&&e.result?.[0]==='string')),'LOGGER_BASELINE actual template conversion');
console.log(JSON.stringify({names,cases,iterationStop:110,scope:'ten lifecycle/delegate roots plus owned timer/rejection callbacks; fullclasses/browser/timer/collection/virtualcallee implementations outsideproof'}));
