// Actual cache methods, native arguments/values and explicit primitive effects.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.cross-cache.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cacheNames=['clearStaleSessionStorage','clearPrePlacement','getPendingOrder','addPendingOrder','removePendingOrder'];const exchangeNames=['addToPrePlacementCache','matchPrePlacement','getPendingHip3Order','addPendingHip3Order','removePendingHip3Order'];const names=[...cacheNames,...exchangeNames.map(n=>'exchange.'+n)];
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='HyperliquidCrossCache');assert(cls);
const methods=cacheNames.map(name=>{const n=cls.members.find(n=>n.name?.getText(source)===name);assert(n,name);return n.getText(source)}).join('\n');
const exchangeFile=path.join(path.dirname(file),"hyperliquid.exchange.ts");const exchangeSource=ts.createSourceFile(exchangeFile,fs.readFileSync(exchangeFile,"utf8"),ts.ScriptTarget.Latest,true);const exchangeClass=exchangeSource.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==="HyperliquidExchange");const exchangeMethods=exchangeNames.map(name=>{const n=exchangeClass.members.find(n=>n.name?.getText(exchangeSource)===name);assert(n,name);return n.getText(exchangeSource)}).join("\n");
let ids=new WeakMap(),tags=new WeakMap(),nextId=1,calls=[],depth=0,globalCase,recording=false;let active=new WeakSet();const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{if(Number.isNaN(n))return'nan';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function id(v){if(!ids.has(v))ids.set(v,nextId++);return ids.get(v);}
function native(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string')return['string',units(v)];if(typeof v==='boolean')return['bool',v];if(typeof v==='number')return['number',bits(v)];if(typeof v==='symbol')return['symbol',idSymbol(v)];
 if(typeof v==='function'||tags.has(v))return['ref',id(v),tags.get(v)||'function'];
 if(active.has(v))return['ref',id(v),'cycle'];active.add(v);let result;
 if(Array.isArray(v))result=['array',id(v),v.map(native)];else
 result=['object',id(v),Object.keys(v).map(k=>{const d=Object.getOwnPropertyDescriptor(v,k);return[units(k),Object.hasOwn(d,'value')?native(d.value):['accessor']]}),Object.getPrototypeOf(v)===null?'null':Object.getPrototypeOf(v)===Object.prototype?'ordinary':'custom'];active.delete(v);return result;
}
let symbols;function idSymbol(v){if(!symbols.has(v))symbols.set(v,nextId++);return symbols.get(v);}
function invoke(name,arg,fn,project=native){const before=depth++;try{const value=fn();if(recording&&before===0)calls.push({name,arg,result:project(value)});return value}catch(error){if(recording&&before===0)calls.push({name,arg,error:native(error)});throw error}finally{depth--;}}
function tag(v,name){tags.set(v,name);return v;}
const bindings={
 __get:(v,k)=>invoke('get:'+k,native(v),()=>v[k]),
 __call:(fn,receiver,...args)=>invoke('call',{function:native(fn),receiver:native(receiver),args:args.map(native)},()=>Reflect.apply(fn,receiver,args)),
 __global:()=>invoke('sessionStorage',[],()=>{if(globalCase.globalThrows)throw globalCase.error;if(globalCase.globalSequence)return globalCase.globalSequence.shift();return globalCase.storage}),
 __key:v=>invoke('propertyKey',native(v),()=>typeof v==='symbol'?v:Reflect.ownKeys({[v]:0})[0]),
 __assign:(object,key,value)=>invoke('set',{object:native(object),key:native(key),value:native(value)},()=>{"use strict";object[key]=value;return true}),
 __object:()=>invoke('object',[],()=>({})),
 __defineData:(object,key,value)=>invoke('defineData',{object:native(object),key:units(key),value:native(value)},()=>{Object.defineProperty(object,key,{value,writable:true,enumerable:true,configurable:true});return true}),
 __now:()=>invoke('now',[],()=>{if(globalCase.clockThrows)throw globalCase.error;return globalCase.now}),
 __keys:v=>invoke('objectKeys',native(v),()=>Object.keys(v)),
 __index:(object,key)=>invoke('index',{object:native(object),key:native(key)},()=>object[key]),
 __delete:(object,key)=>invoke('delete',{object:native(object),key:native(key)},()=>{"use strict";return delete object[key]}),
 __older:(now,timestamp,threshold)=>invoke('olderThan',{now:native(now),timestamp:native(timestamp),milliseconds:native(threshold)},()=>now-timestamp>threshold,v=>v),
 __spread:v=>invoke('spread',native(v),()=>[...v]),
 __greater:v=>invoke('greater500',native(v),()=>v>500,v=>v),
 __pair:entry=>invoke('entryPair',native(entry),()=>{const[k,v]=entry;return[k,v]},v=>v.map(native)),
 __iterate:map=>{
  const iterator=invoke('entries',native(map),()=>tag(map[Symbol.iterator](),'iterator'));
  return {[Symbol.iterator](){return {next(){return invoke('iteratorStep',native(iterator),()=>{const n=iterator.next();if(n.done)return{done:true};return{done:false,value:n.value}},n=>n.done?null:native(n.value))},return(){invoke('closeAfterThrow',native(iterator),()=>iterator.return?.());return{done:true}}}}};
 },
};
function transformer(context){const f=ts.factory;const call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isDeleteExpression(n)&&ts.isElementAccessExpression(n.expression))return call('__delete',[ts.visitNode(n.expression.expression,visit),ts.visitNode(n.expression.argumentExpression,visit)]);
 if(ts.isElementAccessExpression(n))return call('__index',[ts.visitNode(n.expression,visit),ts.visitNode(n.argumentExpression,visit)]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.expression.getText()==='Date')return call('__now',[]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.expression.getText()==='Object'&&n.expression.name.text==='keys')return call('__keys',[ts.visitNode(n.arguments[0],visit)]);
 if(ts.isTypeOfExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='sessionStorage')return f.updateTypeOfExpression(n,call('__global',[]));
 if(ts.isIdentifier(n)&&n.text==='sessionStorage'&&!ts.isTypeOfExpression(n.parent))return call('__global',[]);
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&(ts.isPropertyAccessExpression(n.left)||ts.isElementAccessExpression(n.left))){const key=ts.isPropertyAccessExpression(n.left)?f.createStringLiteral(n.left.name.text):call('__key',[ts.visitNode(n.left.argumentExpression,visit)]);return call('__assign',[ts.visitNode(n.left.expression,visit),key,ts.visitNode(n.right,visit)]);}
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.GreaterThanToken&&ts.isBinaryExpression(n.left)&&n.left.operatorToken.kind===ts.SyntaxKind.MinusToken)return call('__older',[ts.visitNode(n.left.left,visit),ts.visitNode(n.left.right,visit),n.right]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&!['JSON','Date','Object'].includes(n.expression.expression.getText())){const receiver=ts.visitNode(n.expression.expression,visit);return call('__method',[receiver,f.createStringLiteral(n.expression.name.text),...n.arguments.map(a=>ts.visitNode(a,visit))]);}
 if(ts.isPropertyAccessExpression(n)&&!['JSON','Date','Object'].includes(n.expression.getText()))return call('__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
 if(ts.isObjectLiteralExpression(n)){
 const object=f.createIdentifier('__literalObject');const body=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(object,undefined,undefined,call('__object',[]))],ts.NodeFlags.Const))];
 for(const p of n.properties){const key=p.name.text;const value=ts.isShorthandPropertyAssignment(p)?p.name:ts.visitNode(p.initializer,visit);body.push(f.createExpressionStatement(call('__defineData',[object,f.createStringLiteral(key),value])))}
 body.push(f.createReturnStatement(object));return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock(body,true))),undefined,[]);
 }
 if(ts.isArrayLiteralExpression(n)&&n.elements.length===1&&ts.isSpreadElement(n.elements[0]))return call('__spread',[ts.visitNode(n.elements[0].expression,visit)]);
 if(ts.isForOfStatement(n)){
 const entry=f.createIdentifier('__entryValue');const initial=f.createVariableDeclarationList([f.createVariableDeclaration(entry)],ts.NodeFlags.Const);
 const binding=n.initializer.declarations[0].name;const pair=f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(binding,undefined,undefined,ts.isArrayBindingPattern(binding)?call('__pair',[entry]):entry)],ts.NodeFlags.Const));
 const body=f.createBlock([pair,...n.statement.statements.map(s=>ts.visitNode(s,visit))],true);
 return f.updateForOfStatement(n,n.awaitModifier,initial,call('__iterate',[ts.visitNode(n.expression,visit)]),body);
 }
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
// Method helper uses receiver lookup before the arguments. All selected method
// arguments are side-effect-free except JSON.stringify and iterator.next below;
// their special AST rewrite resolves the method first via a lexical thunk.
bindings.__method=(receiver,key,...args)=>bindings.__call(bindings.__get(receiver,key),receiver,...args);
function orderedTransformer(context){const f=ts.factory;const visit=n=>{
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&!['JSON','Date','Object'].includes(n.expression.expression.getText())){
  const receiver=n.expression.expression;const fn=f.createIdentifier('__resolvedFn'),recv=f.createIdentifier('__resolvedReceiver');
  const body=f.createBlock([f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,receiver)],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,f.createCallExpression(f.createIdentifier('__get'),undefined,[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const)),f.createReturnStatement(f.createCallExpression(f.createIdentifier('__call'),undefined,[fn,recv,...n.arguments]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const context={...bindings,exports:{},getCrossCache:()=>invoke('singleton',[],()=>{if(globalCase.singletonThrows)throw globalCase.error;return globalCase.cache}),JSON:{parse:v=>invoke('parse',native(v),()=>JSON.parse(v)),stringify:v=>invoke('stringify',native(v),()=>JSON.stringify(v))},MAX_SEEN_FILLS:500,PRE_PLACEMENT_STORAGE_KEY:'tealstreet_hip3_prePlacement',PENDING_ORDERS_STORAGE_KEY:'tealstreet_hip3_pendingOrders'};
vm.runInNewContext(ts.transpileModule(`class Cache {${methods}}\nclass Exchange {${exchangeMethods}}\nexports.Cache=Cache;exports.Exchange=Exchange;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},transformers:{before:[orderedTransformer,transformer]}}).outputText,context);
const cases=[];
function add(name,label,configure=()=>{},args=[]){
 ids=new WeakMap();tags=new WeakMap();symbols=new Map();nextId=1;calls=[];depth=0;recording=false;
 const error=tag({sentinel:label},'error'),cache=Object.create(context.exports.Cache.prototype);
 cache.workerPrePlacementCache=tag(new Map(),'preMap');cache.workerPendingOrdersCache=tag(new Map(),'pendingMap');
 const g=globalCase={cache,error,record:{},pre:{},now:120000,storage:null};
 cache.getSessionStorage=()=>{if(g.fail==='getSessionStorage')throw error;return g.storage};
 cache.getPrePlacementKey=(coin,side)=>`${coin}|${side}`;
 cache.readPrePlacementCache=()=>{if(g.fail==='readPrePlacementCache')throw error;return g.pre};
 cache.writePrePlacementCache=v=>{if(g.fail==='writePrePlacementCache')throw error;g.writtenPre=v};
 cache.readPendingOrdersCache=()=>{if(g.fail==='readPendingOrdersCache')throw error;return g.record};
 cache.writePendingOrdersCache=v=>{if(g.fail==='writePendingOrdersCache')throw error;g.written=v};
 cache.broadcastCacheUpdate=(...v)=>{if(g.fail==='broadcastCacheUpdate')throw error;g.broadcast=v};
 cache.addPrePlacement=()=>{if(g.fail==='addPrePlacement')throw error;return g.delegateResult};
 cache.matchPrePlacement=()=>{if(g.fail==='matchPrePlacement')throw error;return g.delegateResult};
 configure(g);
 if(name.startsWith('exchange.'))for(const member of ['getPendingOrder','addPendingOrder','removePendingOrder'])cache[member]=()=>{if(g.fail===member)throw error;return g.delegateResult};
 const input=native(cache),initialRecord=native(g.record),initialPre=native(g.pre),inputArgs=args.map(native);
 recording=true;let value,thrown;
 try{value=name.startsWith('exchange.')?context.exports.Exchange.prototype[name.slice(9)].apply({},args):cache[name](...args)}catch(e){thrown=native(e)}
 recording=false;
 cases.push({name,label,input,initialRecord,initialPre,args:inputArgs,calls,...thrown?{error:thrown}:{native:native(value)},finalRecord:native(g.record),finalPre:native(g.pre),written:native(g.written),writtenPre:native(g.writtenPre)});
}
for(const label of ['absent','all removes','get throws','first remove throws','second remove throws','third remove throws','null storage'])add('clearStaleSessionStorage',label,g=>{
 if(label==='get throws')g.fail='getSessionStorage';if(label==='absent'||label==='null storage')return;
 let count=0;g.storage=tag({removeItem(key){count++;if(count===({'first remove throws':1,'second remove throws':2,'third remove throws':3}[label]))throw g.error}},'storage');
});
for(const label of ['missing','empty array truthy','entry','false entry','read throws','write throws','delete throws','broadcast throws','__proto__'])add('clearPrePlacement',label,g=>{
 const key=label==='__proto__'?'__proto__':'BTC|buy';g.pre=label==='missing'?{}:{[key]:label==='false entry'?false:label==='empty array truthy'?[]:{price:91.25,amount:17.5}};
 if(label==='read throws')g.fail='readPrePlacementCache';if(label==='write throws')g.fail='writePrePlacementCache';if(label==='broadcast throws')g.fail='broadcastCacheUpdate';if(label==='delete throws')Object.defineProperty(g.pre,key,{configurable:false});
 if(label==='__proto__')g.cache.getPrePlacementKey=()=>key;
},['BTC','buy']);
for(const [label,delta]of [['fresh',59999],['boundary',60000],['stale return retained',60001],['negative',-1],['NaN',NaN],['Infinity',Infinity]])add('getPendingOrder',label,g=>g.record={order:{type:'limit',price:91.25,timestamp:g.now-delta}},['order']);
add('getPendingOrder','mixed integer keys',g=>g.record={'9':{timestamp:0},'2':{timestamp:119999},order:{timestamp:60000,price:17.5}},['order']);
for(const label of ['missing','empty','read throws','write throws','clock throws','timestamp getter throws','delete throws','record null','timestamp object','requested getter throws'])add('getPendingOrder',label,g=>{
 g.record=label==='empty'?{}:{order:{price:91.25,timestamp:0}};
 if(label==='missing')g.record={other:{timestamp:119999}};if(label==='read throws')g.fail='readPendingOrdersCache';if(label==='write throws')g.fail='writePendingOrdersCache';if(label==='clock throws')g.clockThrows=true;
 if(label==='timestamp getter throws')Object.defineProperty(g.record.order,'timestamp',{get(){throw g.error},enumerable:true});
 if(label==='delete throws')Object.defineProperty(g.record,'order',{configurable:false});if(label==='record null')g.record=null;
 if(label==='timestamp object')g.record.order.timestamp={valueOf(){return 1}};
 if(label==='requested getter throws')Object.defineProperty(g.record,'order',{enumerable:true,get(){throw g.error}});
},['order']);
for(const [label,id,price]of [['base','order',91.25],['overwrite','order',17.5],['lone','\ud800',-0],['prototype','__proto__',Infinity]])add('addPendingOrder',label,g=>g.record={order:{type:'old',price:1,timestamp:0}},[id,'limit',price]);
for(const label of ['clock throws','read throws','write throws','broadcast throws','record null','setter throws','nonwritable slot','proxy rejects'])add('addPendingOrder',label,g=>{if(label==='clock throws')g.clockThrows=true;if(label==='read throws')g.fail='readPendingOrdersCache';if(label==='write throws')g.fail='writePendingOrdersCache';if(label==='broadcast throws')g.fail='broadcastCacheUpdate';if(label==='record null')g.record=null;if(label==='setter throws')Object.defineProperty(g.record,'order',{enumerable:true,set(){throw g.error}});if(label==='proxy rejects')g.record=new Proxy(g.record,{set(){return false}});if(label==='nonwritable slot')Object.defineProperty(g.record,'order',{value:{price:17.5},enumerable:true,writable:false})},['order','limit',91.25]);
for(const label of ['base','missing','lone','read throws','write throws','broadcast throws','delete throws','record null'])add('removePendingOrder',label,g=>{g.record=label==='missing'?{}:{order:{price:91.25,timestamp:0}};if(label==='read throws')g.fail='readPendingOrdersCache';if(label==='write throws')g.fail='writePendingOrdersCache';if(label==='broadcast throws')g.fail='broadcastCacheUpdate';if(label==='delete throws')Object.defineProperty(g.record,'order',{configurable:false});if(label==='record null')g.record=null},[label==='lone'?'\ud800':'order']);
const delegateArgs={addToPrePlacementCache:['BTC','buy','limit',91.25,17.5],matchPrePlacement:['BTC','buy',91.25,17.5],getPendingHip3Order:['order'],addPendingHip3Order:['order','limit',91.25],removePendingHip3Order:['order']};
const delegateMembers={addToPrePlacementCache:'addPrePlacement',matchPrePlacement:'matchPrePlacement',getPendingHip3Order:'getPendingOrder',addPendingHip3Order:'addPendingOrder',removePendingHip3Order:'removePendingOrder'};
for(const name of exchangeNames)for(const label of ['result','undefined','null','singleton throws','method throws','getter throws','lone/signed zero','opaque result identity']){
 const args=delegateArgs[name].slice();if(label==='lone/signed zero'){args[0]='\ud800';if(args.length>2)args[args.length-1]=-0;}
 add('exchange.'+name,label,g=>{g.delegateResult=label==='undefined'?undefined:label==='null'?null:{type:'limit',price:91.25,extra:17.5};if(label==='singleton throws')g.singletonThrows=true;if(label==='method throws')g.fail=delegateMembers[name];if(label==='getter throws')Object.defineProperty(g.cache,delegateMembers[name],{get(){throw g.error},configurable:false})},args);
}
for(const args of [['BTC','buy'],['BTC','buy',91.25]])add('exchange.matchPrePlacement','omitted optional arguments',g=>g.delegateResult={type:'limit',price:17.5},args);
assert.equal(cases.length,95,'independent7+9+17+12+8+42');
console.log(JSON.stringify({names,cases,iterationStop:94,scope:'ten lifecycle/delegation source methods; explicit opaque native primitive/callee bindings; no class/constructor/singleton implementation claim'}));
