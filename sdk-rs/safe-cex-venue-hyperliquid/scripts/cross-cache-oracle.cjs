// Actual cache methods, native arguments/values and explicit primitive effects.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.cross-cache.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['getSessionStorage','broadcastCacheUpdate','getPrePlacementKey','readPrePlacementCache','writePrePlacementCache','readPendingOrdersCache','writePendingOrdersCache','hasSeenFill','markFillSeen','dispose'];
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='HyperliquidCrossCache');assert(cls);
const methods=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(source)===name);assert(n,name);return n.getText(source)}).join('\n');
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
 __object:pairs=>{const value=invoke('object',[],()=>({}));for(const[k,v]of pairs)invoke('defineData',{object:native(value),key:units(k),value:native(v)},()=>{Object.defineProperty(value,k,{value:v,writable:true,enumerable:true,configurable:true});return true});return value;},
 __spread:v=>invoke('spread',native(v),()=>[...v]),
 __greater:v=>invoke('greater500',native(v),()=>v>500,v=>v),
 __pair:entry=>invoke('entryPair',native(entry),()=>{const[k,v]=entry;return[k,v]},v=>v.map(native)),
 __iterate:map=>{
  const iterator=invoke('entries',native(map),()=>tag(map[Symbol.iterator](),'iterator'));
  return {[Symbol.iterator](){return {next(){return invoke('iteratorStep',native(iterator),()=>{const n=iterator.next();if(n.done)return{done:true};return{done:false,value:n.value}},n=>n.done?null:native(n.value))},return(){invoke('closeAfterThrow',native(iterator),()=>iterator.return?.());return{done:true}}}}};
 },
};
function transformer(context){const f=ts.factory;const call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isTypeOfExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='sessionStorage')return f.updateTypeOfExpression(n,call('__global',[]));
 if(ts.isIdentifier(n)&&n.text==='sessionStorage'&&!ts.isTypeOfExpression(n.parent))return call('__global',[]);
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&(ts.isPropertyAccessExpression(n.left)||ts.isElementAccessExpression(n.left))){const key=ts.isPropertyAccessExpression(n.left)?f.createStringLiteral(n.left.name.text):call('__key',[ts.visitNode(n.left.argumentExpression,visit)]);return call('__assign',[ts.visitNode(n.left.expression,visit),key,ts.visitNode(n.right,visit)]);}
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.GreaterThanToken&&n.right.getText()==='MAX_SEEN_FILLS')return call('__greater',[ts.visitNode(n.left,visit)]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&!['JSON'].includes(n.expression.expression.getText())){const receiver=ts.visitNode(n.expression.expression,visit);return call('__method',[receiver,f.createStringLiteral(n.expression.name.text),...n.arguments.map(a=>ts.visitNode(a,visit))]);}
 if(ts.isPropertyAccessExpression(n)&&!['JSON'].includes(n.expression.getText()))return call('__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
 if(ts.isObjectLiteralExpression(n))return call('__object',[f.createArrayLiteralExpression(n.properties.map(p=>{assert(ts.isShorthandPropertyAssignment(p));return f.createArrayLiteralExpression([f.createStringLiteral(p.name.text),p.name])}))]);
 if(ts.isArrayLiteralExpression(n)&&n.elements.length===1&&ts.isSpreadElement(n.elements[0]))return call('__spread',[ts.visitNode(n.elements[0].expression,visit)]);
 if(ts.isForOfStatement(n)){
 const entry=f.createIdentifier('__entryValue');const initial=f.createVariableDeclarationList([f.createVariableDeclaration(entry)],ts.NodeFlags.Const);
 const binding=n.initializer.declarations[0].name;const pair=f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(binding,undefined,undefined,call('__pair',[entry]))],ts.NodeFlags.Const));
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
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.expression.getText()!=='JSON'){
  const receiver=n.expression.expression;const fn=f.createIdentifier('__resolvedFn'),recv=f.createIdentifier('__resolvedReceiver');
  const body=f.createBlock([f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,receiver)],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,f.createCallExpression(f.createIdentifier('__get'),undefined,[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const)),f.createReturnStatement(f.createCallExpression(f.createIdentifier('__call'),undefined,[fn,recv,...n.arguments]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const context={...bindings,exports:{},JSON:{parse:v=>invoke('parse',native(v),()=>JSON.parse(v)),stringify:v=>invoke('stringify',native(v),()=>JSON.stringify(v))},MAX_SEEN_FILLS:500,PRE_PLACEMENT_STORAGE_KEY:'tealstreet_hip3_prePlacement',PENDING_ORDERS_STORAGE_KEY:'tealstreet_hip3_pendingOrders'};
vm.runInNewContext(ts.transpileModule(`class Cache {${methods}}\nexports.Cache=Cache;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},transformers:{before:[orderedTransformer,transformer]}}).outputText,context);
const cases=[];function add(name,label,configure=()=>{},args=[]){ids=new WeakMap();tags=new WeakMap();symbols=new Map();nextId=1;calls=[];depth=0;recording=false;const error=tag({sentinel:label},'error');
 const cache=Object.create(context.exports.Cache.prototype);cache.workerPrePlacementCache=tag(new Map(),'preMap');cache.workerPendingOrdersCache=tag(new Map(),'pendingMap');cache.workerSeenFillsSet=tag(new Set(),'seenSet');cache.orderCacheBroadcast=null;
 globalCase={cache,error,storage:undefined};configure(globalCase);const initialSeen=Array.from(cache.workerSeenFillsSet,units);const input=native(cache),argumentsNative=args.map(native);recording=true;let value,thrown;try{value=cache[name](...args)}catch(e){thrown=native(e)}recording=false;
 cases.push({name,label,input,args:argumentsNative,calls,...thrown?{error:thrown}:{native:native(value)},finalCache:native(cache),initialSeen,finalSeen:Array.from(cache.workerSeenFillsSet,units)});
}
const storage=(g,value,fail)=>g.storage=tag({getItem(key){if(fail==='get')throw g.error;return value},setItem(key,text){if(fail==='set')throw g.error}},'storage');
for(const label of ['absent','present','throw','secondFresh','secondThrow'])add('getSessionStorage',label,g=>{if(label==='present')storage(g,'{}');if(label==='throw')g.globalThrows=true;if(label==='secondFresh')g.globalSequence=[tag({},'firstStorage'),null];if(label==='secondThrow'){let n=0;Object.defineProperty(g,'storage',{get(){if(n++)throw g.error;return tag({},'firstStorage')}})}});
for(const [coin,side]of [['BTC','buy'],['',''],['A|B','sell'],['\ud800','\udfff']])add('getPrePlacementKey','units',()=>{},[coin,side]);
for(const name of ['readPrePlacementCache','readPendingOrdersCache']){
 for(const value of ['{"BTC":{"price":91.25,"amount":17.5}}','[-0,1e400]','null','false','17','"\\ud800"','','bad',Symbol('bad')])add(name,'storage value',g=>storage(g,value));
 add(name,'get failure',g=>storage(g,'{}','get'));
 add(name,'method getter failure',g=>Object.defineProperty(g.cache,'getSessionStorage',{get(){throw g.error}}));
 add(name,'worker empty');
 add(name,'worker entries',g=>{const entry={price:91.25,amount:17.5,text:'\ud800'};g.cache.workerPrePlacementCache.set('BTC',[entry]);g.cache.workerPrePlacementCache.set('__proto__',[entry]);g.cache.workerPrePlacementCache.set('2',[entry]);g.cache.workerPendingOrdersCache.set('BTC',entry);g.cache.workerPendingOrdersCache.set('__proto__',entry);g.cache.workerPendingOrdersCache.set('2',entry)});
 add(name,'worker iterator failure',g=>{const map=name==='readPrePlacementCache'?g.cache.workerPrePlacementCache:g.cache.workerPendingOrdersCache;map[Symbol.iterator]=()=>{throw g.error}});
 add(name,'worker body failure',g=>{const map=name==='readPrePlacementCache'?g.cache.workerPrePlacementCache:g.cache.workerPendingOrdersCache;map.set({toString(){throw g.error}},name==='readPrePlacementCache'?[]:{});map[Symbol.iterator]=function*(){try{yield* Map.prototype.entries.call(this)}finally{}}});
}
for(const name of ['writePrePlacementCache','writePendingOrdersCache']){
 for(const value of [{price:91.25,amount:17.5,text:'\ud800'},undefined,{x:-0,y:Infinity},{toJSON(){throw Error('json failure')}},(()=>{const v={};v.self=v;return v})()])add(name,'write native',g=>storage(g,''),[value]);
 add(name,'absent',()=>{},[{}]);add(name,'set failure',g=>storage(g,'','set'),[{}]);add(name,'method getter failure',g=>Object.defineProperty(g.cache,'getSessionStorage',{get(){throw g.error}}),[{}]);
}
for(const label of ['none','send','sendThrows','secondGetterThrows','firstGetterThrows'])add('broadcastCacheUpdate',label,g=>{if(label==='none')return;const channel=tag({postMessage(){if(label==='sendThrows')throw g.error}},'channel');g.cache.orderCacheBroadcast=channel;if(label.includes('Getter')){let n=0;Object.defineProperty(g.cache,'orderCacheBroadcast',{enumerable:true,get(){if(label==='firstGetterThrows'||n++)throw g.error;return channel}})}},['addPending','\ud800',{price:91.25,amount:17.5}]);
for(const label of ['missing','present','throw'])add('hasSeenFill',label,g=>{g.cache.workerSeenFillsSet.add('seen');if(label==='throw')g.cache.workerSeenFillsSet.has=()=>{throw g.error}},[label==='present'?'seen':'\ud800']);
for(const count of [0,499,500,501])add('markFillSeen','size '+count,g=>{for(let i=0;i<count;i++)g.cache.workerSeenFillsSet.add('fill'+i)},['new']);
add('markFillSeen','existing does not reorder',g=>{for(let i=0;i<501;i++)g.cache.workerSeenFillsSet.add('fill'+i)},['fill0']);
add('markFillSeen','add throws',g=>g.cache.workerSeenFillsSet.add=()=>{throw g.error},['new']);
add('markFillSeen','broadcast throws',g=>g.cache.broadcastCacheUpdate=()=>{throw g.error},['new']);
for(const label of ['none','close','closeThrows','firstGetterThrows','secondGetterThrows','setterThrows'])add('dispose',label,g=>{if(label==='none')return;const channel=tag({close(){if(label==='closeThrows')throw g.error}},'channel');g.cache.orderCacheBroadcast=channel;if(label.includes('Getter')){let n=0;Object.defineProperty(g.cache,'orderCacheBroadcast',{enumerable:true,get(){if(label==='firstGetterThrows'||n++)throw g.error;return channel},set(){}})}if(label==='setterThrows')Object.defineProperty(g.cache,'orderCacheBroadcast',{enumerable:true,get:()=>channel,set(){throw g.error}})});
for(const name of ['readPrePlacementCache','readPendingOrdersCache'])for(const closeThrows of [false,true])add(name,'bad entry closes outer iterator '+closeThrows,g=>{const map=name==='readPrePlacementCache'?g.cache.workerPrePlacementCache:g.cache.workerPendingOrdersCache;map[Symbol.iterator]=()=>({next:()=>({done:false,value:7}),return(){if(closeThrows)throw g.error;return{done:true}}})});
assert.equal(cases.length,80,'fixed independent76 plus4 badentry/close witnesses');
console.log(JSON.stringify({names,cases,iterationStop:79,scope:'ten source methods; native values/arguments, original library/browser intrinsic bindings, no constructor/class claim'}));
