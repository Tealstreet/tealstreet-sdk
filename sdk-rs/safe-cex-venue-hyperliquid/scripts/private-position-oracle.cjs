// Actual cache methods, native arguments/values and explicit primitive effects.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-private.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['getFillPositionSide','safeParseNumber','trackProcessedRealizedPnlFill','getRealizedPnlKey','resolveSymbolFromCoin','setRealizedPnlForPosition','incrementRealizedPnlForPosition','getRealizedPnlForPosition','resolvePositionMode','extractCoreBalanceSummary','getMessageTimestamp'];
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='HyperliquidPrivateWebSocket');assert(cls);
const methods=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(source)===name);assert(n,name);return n.getText(source)}).join('\n');
const threshold=cls.members.find(n=>n.name?.getText(source)==='MAX_TRACKED_REALIZED_PNL_FILL_IDS').getText(source);
const typesFile=path.resolve(path.dirname(file),'../../types.ts'),typesSource=ts.createSourceFile(typesFile,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true);
const enums=['PositionSide','PositionMode'].map(name=>typesSource.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text===name).getText(typesSource)).join('\n');
let ids=new WeakMap(),tags=new WeakMap(),nextId=1,calls=[],depth=0,globalCase,recording=false;let active=new WeakSet();const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{if(Number.isNaN(n))return'nan';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function id(v){if(!ids.has(v))ids.set(v,nextId++);return ids.get(v);}
function native(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string')return['string',units(v)];if(typeof v==='boolean')return['bool',v];if(typeof v==='number')return['number',bits(v)];if(typeof v==='bigint')return['bigint',units(String(v))];if(typeof v==='symbol')return['symbol',idSymbol(v)];
 if(typeof v==='function'||tags.has(v))return['ref',id(v),tags.get(v)||'function'];
 if(active.has(v))return['ref',id(v),'cycle'];active.add(v);let result;
 if(Array.isArray(v))result=['array',id(v),v.map(native)];else
 result=['object',id(v),Object.keys(v).map(k=>{const d=Object.getOwnPropertyDescriptor(v,k);return[units(k),Object.hasOwn(d,'value')?native(d.value):['accessor']]}),Object.getPrototypeOf(v)===null?'null':Object.getPrototypeOf(v)===Object.prototype?'ordinary':'custom'];active.delete(v);return result;
}
let symbols;function idSymbol(v){if(!symbols.has(v))symbols.set(v,nextId++);return symbols.get(v);}
function invoke(name,arg,fn,project=native){const before=depth++;try{const value=fn();if(recording&&before===0)calls.push({name,arg,result:project(value)});return value}catch(error){if(recording&&before===0)calls.push({name,arg,error:native(error)});throw error}finally{depth--;}}
function tag(v,name){tags.set(v,name);return v;}
const bindings={
 __optional:(v,k)=>v==null?undefined:bindings.__get(v,k),
 __optionalIndex:(v,k)=>v==null?undefined:bindings.__index(v,k),
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
 __number:v=>invoke('number',native(v),()=>Number(v)),
 __lower:v=>invoke('lowercase',units(v),()=>String.prototype.toLowerCase.call(v)),
 __classLimit:()=>invoke('classLimit',[],()=>context.exports.Socket.MAX_TRACKED_REALIZED_PNL_FILL_IDS),
 __greater:(v,limit)=>invoke('greaterThan',{value:native(v),limit:native(limit)},()=>v>limit,v=>v),
 __add:(left,right)=>invoke('add',{left:native(left),right:native(right)},()=>left+right),
 __pair:entry=>invoke('entryPair',native(entry),()=>{const[k,v]=entry;return[k,v]},v=>v.map(native)),
 __iterate:map=>{
  const iterator=invoke('entries',native(map),()=>tag(map[Symbol.iterator](),'iterator'));
  return {[Symbol.iterator](){return {next(){return invoke('iteratorStep',native(iterator),()=>{const n=iterator.next();if(n.done)return{done:true};return{done:false,value:n.value}},n=>n.done?null:native(n.value))},return(){invoke('closeAfterThrow',native(iterator),()=>iterator.return?.());return{done:true}}}}};
 },
};
function transformer(context){const f=ts.factory;const call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.name.text==='includes')return f.updateCallExpression(n,f.updatePropertyAccessExpression(n.expression,ts.visitNode(n.expression.expression,visit),n.expression.name),n.typeArguments,n.arguments.map(a=>ts.visitNode(a,visit)));
 if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='Number')return call('__number',n.arguments.map(a=>ts.visitNode(a,visit)));
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.name.text==='toLowerCase')return call('__lower',[ts.visitNode(n.expression.expression,visit)]);
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.PlusToken)return call('__add',[ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);
 if(ts.isDeleteExpression(n)&&ts.isElementAccessExpression(n.expression))return call('__delete',[ts.visitNode(n.expression.expression,visit),ts.visitNode(n.expression.argumentExpression,visit)]);
 if(ts.isElementAccessExpression(n))return call(n.questionDotToken?'__optionalIndex':'__index',[ts.visitNode(n.expression,visit),ts.visitNode(n.argumentExpression,visit)]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.expression.getText()==='Date')return call('__now',[]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&n.expression.expression.getText()==='Object'&&n.expression.name.text==='keys')return call('__keys',[ts.visitNode(n.arguments[0],visit)]);
 if(ts.isTypeOfExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='sessionStorage')return f.updateTypeOfExpression(n,call('__global',[]));
 if(ts.isIdentifier(n)&&n.text==='sessionStorage'&&!ts.isTypeOfExpression(n.parent))return call('__global',[]);
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&(ts.isPropertyAccessExpression(n.left)||ts.isElementAccessExpression(n.left))){const key=ts.isPropertyAccessExpression(n.left)?f.createStringLiteral(n.left.name.text):call('__key',[ts.visitNode(n.left.argumentExpression,visit)]);return call('__assign',[ts.visitNode(n.left.expression,visit),key,ts.visitNode(n.right,visit)]);}
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.GreaterThanToken)return call('__greater',[ts.visitNode(n.left,visit),ts.isPropertyAccessExpression(n.right)&&ts.isIdentifier(n.right.expression)&&n.right.expression.text==='HyperliquidPrivateWebSocket'&&n.right.name.text==='MAX_TRACKED_REALIZED_PNL_FILL_IDS'?call('__classLimit',[]):n.right]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&!['toLowerCase','includes'].includes(n.expression.name.text)&&!['JSON','Date','Object','Number','PositionSide','PositionMode','HyperliquidPrivateWebSocket'].includes(n.expression.expression.getText())){const receiver=ts.visitNode(n.expression.expression,visit);return call('__method',[receiver,f.createStringLiteral(n.expression.name.text),...n.arguments.map(a=>ts.visitNode(a,visit))]);}
 if(ts.isPropertyAccessExpression(n)&&!['JSON','Date','Object','Number','PositionSide','PositionMode','HyperliquidPrivateWebSocket'].includes(n.expression.getText()))return call(n.questionDotToken?'__optional':'__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
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
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&!['toLowerCase','includes'].includes(n.expression.name.text)&&!['JSON','Date','Object','Number','PositionSide','PositionMode','HyperliquidPrivateWebSocket'].includes(n.expression.expression.getText())){
  const receiver=n.expression.expression;const fn=f.createIdentifier('__resolvedFn'),recv=f.createIdentifier('__resolvedReceiver');
  const body=f.createBlock([f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,receiver)],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,f.createCallExpression(f.createIdentifier('__get'),undefined,[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const)),f.createReturnStatement(f.createCallExpression(f.createIdentifier('__call'),undefined,[fn,recv,...n.arguments]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const context={...bindings,exports:{},String:v=>invoke('string',native(v),()=>String(v)),Number,getCrossCache:()=>invoke('singleton',[],()=>{if(globalCase.singletonThrows)throw globalCase.error;return globalCase.cache}),JSON:{parse:v=>invoke('parse',native(v),()=>JSON.parse(v)),stringify:v=>invoke('stringify',native(v),()=>JSON.stringify(v))},MAX_SEEN_FILLS:500,PRE_PLACEMENT_STORAGE_KEY:'tealstreet_hip3_prePlacement',PENDING_ORDERS_STORAGE_KEY:'tealstreet_hip3_pendingOrders'};
vm.runInNewContext(ts.transpileModule(`${enums}\nclass HyperliquidPrivateWebSocket {${threshold}\n${methods}}\nexports.Socket=HyperliquidPrivateWebSocket;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},transformers:{before:[orderedTransformer,transformer]}}).outputText,context);
const cases=[];
function add(name,label,args=[],configure=()=>{}){
 ids=new WeakMap();tags=new WeakMap();symbols=new Map();active=new WeakSet();nextId=1;calls=[];depth=0;recording=false;
 const error=tag({sentinel:label},'error'),socket=Object.create(context.exports.Socket.prototype);
 const queue=tag([],'queue'),seen=tag(new Set(),'seenSet'),pnl=tag(new Map(),'pnlMap'),lower=tag(new Map(),'lowerMap');
 socket.processedRealizedPnlFillIds=seen;socket.processedRealizedPnlFillQueue=queue;socket.realizedPnlByPositionKey=pnl;socket.parent={marketsLowerCaseMap:lower};
 Object.defineProperty(context.exports.Socket,"MAX_TRACKED_REALIZED_PNL_FILL_IDS",{value:20000,writable:true,enumerable:true,configurable:true});
 const g=globalCase={socket,error,queue,seen,pnl,lower};configure(g);
 const initialPnl=Array.from(pnl,([k,v])=>[native(k),native(v)]),initialSeen=Array.from(seen,units),queueLength=queue.length,queueFirst=queue[0],queueLast=queue.at(-1),input=native(socket),inputArgs=args.map(native);
 recording=true;let value,thrown;try{value=socket[name](...args)}catch(e){thrown=native(e)}recording=false;
 const result=name==='extractCoreBalanceSummary'&&value?['summary',bits(value.accountValue),bits(value.totalMarginUsed),units(value.sourceField)]:native(value);
 cases.push({name,label,input,args:inputArgs,calls,...thrown?{error:thrown}:{native:result},initialPnl,initialSeen,queueLength,queueFirst:native(queueFirst),queueLast:native(queueLast),finalSeen:Array.from(seen,units),finalQueue:{length:queue.length,first:native(queue[0]),last:native(queue.at(-1))},finalPnl:Array.from(pnl,([k,v])=>[native(k),native(v)])});
}
const dirs=['open long','close short','long short','LONG','shortlong','longshort','close','', '\ud800long\udfff','opening longish','short','💚'];assert.equal(dirs.length,12);for(const v of dirs)add('getFillPositionSide','direction',[v]);
const scalarValues=[undefined,null,false,true,0,-0,91.25,NaN,Infinity,-Infinity,'17.5','17.5tail','\ufeff17.5','',[],[17.5],{},Symbol('number'),9007199254740993n,{valueOf(){return -0}},{valueOf(){throw Error('scalar coercion')}}];assert.equal(scalarValues.length,21);for(const v of scalarValues)add('safeParseNumber','number coercion',[v]);
for(const length of [0,19999,20000,20001])add('trackProcessedRealizedPnlFill','queue boundary '+length,['fresh'],g=>{g.queue.push(...Array(length).fill('queued'));if(length)g.seen.add('queued')});
add('trackProcessedRealizedPnlFill','empty oldest remains seen',['fresh'],g=>{g.queue.push('',...Array(20000).fill('queued'));g.seen.add('');g.seen.add('queued')});
add('trackProcessedRealizedPnlFill','duplicate queue ID removes set ID',['queued'],g=>{g.queue.push(...Array(20000).fill('queued'));g.seen.add('queued')});
for(const operation of ['add','push','shift','delete'])add('trackProcessedRealizedPnlFill','primitive throws '+operation,['fresh'],g=>{g.queue.push(...Array(20000).fill('queued'));g.seen.add('queued');(operation==='add'||operation==='delete'?g.seen:g.queue)[operation]=()=>{throw g.error}});
for(const limit of [0,1])add('trackProcessedRealizedPnlFill','mutable class limit '+limit,['fresh'],g=>{context.exports.Socket.MAX_TRACKED_REALIZED_PNL_FILL_IDS=limit;if(limit){g.queue.push('queued');g.seen.add('queued')}});
add('trackProcessedRealizedPnlFill','class limit getter throws',['fresh'],g=>Object.defineProperty(context.exports.Socket,'MAX_TRACKED_REALIZED_PNL_FILL_IDS',{get(){throw g.error},configurable:true}));
const keys=[['BTC/USDC','long'],['BTC:long','short'],['',''],['\ud800','\udfff']];assert.equal(keys.length,4);for(const pair of keys)add('getRealizedPnlKey','key units',pair);
for(const label of ['exact','missing','dex lowercase','dex missing','empty symbol','false market','read throws','parent throws','lower get throws','Unicode dex']){
 const coin=label==='Unicode dex'?'İΟΣ:ASSET':label.includes('dex')||label==='parent throws'||label==='lower get throws'?'XYZ:ASSET':'BTC';const markets={};
 add('resolveSymbolFromCoin',label,[coin,markets],g=>{if(label==='exact')markets[coin]={symbol:'BTC/USDC'};if(label==='false market')markets[coin]=false;if(label==='empty symbol')markets[coin]={symbol:''};if(label==='dex lowercase'||label==='Unicode dex')g.lower.set(coin.toLowerCase(),{symbol:'DEX/USDC'});if(label==='read throws')Object.defineProperty(markets,coin,{enumerable:true,get(){throw g.error}});if(label==='parent throws')Object.defineProperty(g.socket,'parent',{enumerable:true,get(){throw g.error}});if(label==='lower get throws')g.lower.get=()=>{throw g.error}});
}
for(const name of ['setRealizedPnlForPosition','incrementRealizedPnlForPosition','getRealizedPnlForPosition']){
 const values=[undefined,0,-0,17.5,NaN,Infinity,'3'];assert.equal(values.length,7);
 for(const old of values){const args=name==='getRealizedPnlForPosition'?['BTC/USDC','long']:['BTC','long',91.25,{BTC:{symbol:'BTC/USDC'}}];add(name,'pnl value',args,g=>{if(old!==undefined)g.pnl.set('BTC/USDC:long',old)})}
 for(const operation of ['get','set','getRealizedPnlKey','resolveSymbolFromCoin']){
  const args=name==='getRealizedPnlForPosition'?['BTC/USDC','long']:['BTC','long',91.25,{BTC:{symbol:'BTC/USDC'}}];add(name,'callee throws '+operation,args,g=>{if(operation==='get'||operation==='set')g.pnl[operation]=()=>{throw g.error};else g.socket[operation]=()=>{throw g.error}});
 }
}
const modes=[undefined,null,{}, {type:'hedged'},{type:'TWOWAY'},{type:'oneway'},{type:0},{type:Symbol('mode')},{type:'\ud800hedged'},{type:{toString(){return 'HeDgEd'}}},{get type(){throw Error('mode getter')}},{type:{toString(){throw Error('mode coercion')}}}];assert.equal(modes.length,12);for(const v of modes)add('resolvePositionMode','mode',[v]);
const balances=[undefined,null,{}, {marginSummary:{accountValue:'91.25',totalMarginUsed:'17.5'}},{crossMarginSummary:{accountValue:17.5,totalMarginUsed:91.25}},{marginSummary:{accountValue:-0,totalMarginUsed:-0}},{marginSummary:{accountValue:Infinity,totalMarginUsed:17.5},crossMarginSummary:{accountValue:1,totalMarginUsed:2}},{marginSummary:{accountValue:17.5,totalMarginUsed:NaN}},{marginSummary:{},crossMarginSummary:{accountValue:1,totalMarginUsed:2}},{marginSummary:false,crossMarginSummary:{accountValue:91.25,totalMarginUsed:17.5}},{marginSummary:{accountValue:Symbol('balance'),totalMarginUsed:17.5}},{get marginSummary(){throw Error('balance selection')}}];assert.equal(balances.length,12);for(const v of balances)add('extractCoreBalanceSummary','balance',[v]);
let summaryRead=0;add('extractCoreBalanceSummary','fresh summary reads',[{get marginSummary(){return ++summaryRead===1?{}:summaryRead===2?{accountValue:91.25}:{totalMarginUsed:17.5}}}]);
const times=[undefined,null,{}, {serverTime:91.25,time:17.5},{serverTime:null,time:17.5},{serverTime:0,time:17.5},{serverTime:'',time:17.5},{serverTime:NaN,time:17.5},{time:Infinity},{clearinghouseState:{time:91.25}},{time:-0},{time:-17.5},{serverTime:'91.25'},{serverTime:Symbol('time')},{get serverTime(){throw Error('time getter')}},{serverTime:null,time:null,clearinghouseState:null},{serverTime:1e400},{serverTime:1e-300}];assert.equal(times.length,18);for(const v of times)add('getMessageTimestamp','time',[v]);
assert.equal(cases.length,136,'independent12+21+13+4+10+33+12+13+18');
console.log(JSON.stringify({names,cases,iterationStop:135,scope:'eleven private position/scalar methods; native values/state/arguments and real primitive/callee results; classes/WS/live runtime not claimed'}));
