'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const sourceNames={public:['sendWithDiagnostics','emitReceiveDiagnostics'],private:['sendWithDiagnostics','emitReceiveDiagnostics','subscribeSystemTopics','subscribeAllActiveAssetData'],spot_public:['subscribePublicTopics','subscribeUserTopics'],spot_private:['subscribeSystemTopics','subscribeCoins']};
let g,ids,tags,nextId,calls;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const tag=(v,t)=>(tags.set(v,t),v);
function native(v){
 if(v===undefined)return ['undefined'];if(v===null)return ['null'];
 if(typeof v==='boolean')return ['bool',v];if(typeof v==='string')return ['string',units(v)];
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return ['number',Number.isNaN(v)?'nan':b.toString('hex')];}
 if(!ids.has(v))ids.set(v,nextId++);return ['ref',ids.get(v),tags.get(v)||(v?.name?.endsWith('Error')?'error:'+v.name:typeof v)];
}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(e0){e.throw=native(e0);throw e0;}}
const globals={};
function literalTree(v){if(tags.get(v)!=='literal')return native(v);return ['literal',native(v),Object.keys(v).map(k=>[units(k),literalTree(v[k])])];}
const bind={
 __global:name=>op('global',name==='systemSubscriptionTypes'?{name,venue:g.kind.startsWith('spot_')?'spot':'perp'}:{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish property access');return Reflect.get(Object(object),name,object);}),
 __call:(fn,receiver,...args)=>{const input={function:native(fn),receiver:native(receiver),args:args.map(native)};if(tags.get(fn)==='stringify'||tags.get(fn)?.startsWith('emitWsDiagnostic'))input.payload=literalTree(args.at(-1));return op('call',input,()=>{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;return Reflect.apply(fn,receiver,args);});},
 __newObject:()=>op('object',{},()=>tag({},'literal')),
 __define:(object,name,value)=>op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});}),
 __spreadInto:(object,value)=>op('spread',{target:native(object),source:native(value),payload:literalTree(value)},()=>{for(const key of Reflect.ownKeys(value)){const desc=Object.getOwnPropertyDescriptor(value,key);if(desc?.enumerable)Object.defineProperty(object,key,{value:value[key],writable:true,enumerable:true,configurable:true});}}),
 __formatter:(scope,fn)=>op('formatter',{scope},()=>tag(fn,'formatter')),
 __triple:value=>{const entry={op:'triple',args:{value:native(value)}};calls.push(entry);try{const[a,b,c]=value;entry.result=[native(a),native(b),native(c)];return[a,b,c];}catch(e){entry.throw=native(e);throw e;}},
 __iterate:value=>op('iterator',{value:native(value)},()=>{
  const real=value[Symbol.iterator]();if(real===null||(typeof real!=='object'&&typeof real!=='function'))throw new TypeError('iterator must be an object');const nextMethod=real.next;let nextCount=0;
  const iterator=tag({[Symbol.iterator](){return this;},next(){const entry={op:'next',args:{iterator:native(iterator)}};calls.push(entry);try{if(++nextCount===g.throwNext)throw g.error;const step=Reflect.apply(nextMethod,real,[]);if(step===null||(typeof step!=='object'&&typeof step!=='function'))throw new TypeError('iterator result must be an object');const done=!!step.done;const value=done?undefined:step.value;entry.result=done?['done']:native(value);return {done,value};}catch(e){entry.throw=native(e);throw e;}},return(){return op('close',{iterator:native(iterator)},()=>{if(g.throwClose)throw g.closeError;const close=real.return;if(close==null)return{};const result=Reflect.apply(close,real,[]);if(result===null||(typeof result!=='object'&&typeof result!=='function'))throw new TypeError('iterator return must be an object');return result;});}},'iterator');
  return iterator;
 }),
};
const globalNames=new Set(['performance','JSON','Object','flatten','systemSubscriptionTypes','formatHyperliquidWsDiagnostic','shouldEmitHyperliquidWsSendDiagnostic','shouldEmitHyperliquidWsReceiveDiagnostic']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isForOfStatement(n))return f.updateForOfStatement(n,n.awaitModifier,n.initializer,call('__iterate',[ts.visitNode(n.expression,visit)]),ts.visitNode(n.statement,visit));
 if(ts.isVariableDeclaration(n)&&ts.isArrayBindingPattern(n.name))return f.updateVariableDeclaration(n,n.name,n.exclamationToken,n.type,call('__triple',[ts.visitNode(n.initializer,visit)]));
 if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&globalNames.has(n.expression.text))return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...n.arguments.map(a=>ts.visitNode(a,visit))]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
  const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
  const vars=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,ts.visitNode(n.expression.expression,visit))],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,call('__get',[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const))];
  const body=f.createBlock([...vars,f.createReturnStatement(call('__call',[fn,recv,...n.arguments.map(a=>ts.visitNode(a,visit))]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 if(ts.isElementAccessExpression(n))return call('__get',[ts.visitNode(n.expression,visit),ts.visitNode(n.argumentExpression,visit)]);
 if(ts.isPropertyAccessExpression(n))return call('__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
 if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
 if(ts.isObjectLiteralExpression(n)){
  const object=f.createIdentifier('__literal');const statements=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(object,undefined,undefined,call('__newObject',[]))],ts.NodeFlags.Const))];
  for(const p of n.properties){
   if(ts.isSpreadAssignment(p)){statements.push(f.createExpressionStatement(call('__spreadInto',[object,ts.visitNode(p.expression,visit)])));continue;}
   const key=p.name.getText().replace(/^['"]|['"]$/g,'');
   let value=ts.isShorthandPropertyAssignment(p)?ts.visitNode(p.name,visit):ts.visitNode(p.initializer,visit);
   if(key==='format'&&ts.isArrowFunction(p.initializer))value=call('__formatter',[p.initializer.body.arguments[0],value]);
   statements.push(f.createExpressionStatement(call('__define',[object,f.createStringLiteral(key),value])));
  }
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(object)],true))),undefined,[]);
 }
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const diagnostics={exports:{}};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-diagnostics.ts'),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,diagnostics);
const classes={};
for(const [kind,names] of Object.entries(sourceNames)){
 const stem=kind.startsWith('spot_')?'hyperliquidspot':'hyperliquid',side=kind.endsWith('public')?'public':'private';
 const file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.ws-${side}.ts`),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
 const methods=names.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const transformed=ts.transform(m.initializer,[transformer]);const init=printer.printNode(ts.EmitHint.Expression,transformed.transformed[0],ast);transformed.dispose();return `${name} = ${init};`;}).join('\n');
 const constantsFile=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.constants.ts`),constantsSource=fs.readFileSync(constantsFile,'utf8'),constantsAst=ts.createSourceFile(constantsFile,constantsSource,ts.ScriptTarget.Latest,true);
 const types=constantsAst.statements.filter(ts.isVariableStatement).flatMap(n=>n.declarationList.declarations).find(n=>n.name.getText(constantsAst)==='systemSubscriptionTypes');assert(types);
 const context={...bind,exports:{}};
 vm.runInNewContext(ts.transpileModule(`const systemSubscriptionTypes=${types.initializer.getText(constantsAst)};class ${cls.name.text}{${methods}}\nexports.Socket=${cls.name.text};exports.types=systemSubscriptionTypes;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
 classes[kind]=context.exports;
}
const coreTypes=['notification','orderUpdates','userEvents','userFills','userFundings','userNonFundingLedgerUpdates'],spotTypes=['notification','spotClearinghouseState','orderUpdates','userEvents','userFills','userFundings','userNonFundingLedgerUpdates'];
assert.deepEqual(Array.from(classes.private.types),coreTypes);assert.deepEqual(Array.from(classes.spot_private.types),spotTypes);
const cases=[];
function add(kind,name,label,args=[],config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];
 const socket=tag(new classes[kind].Socket(),'socket'),error=tag({label},'error'),closeError=tag({label:'close'},'closeError');
 g={kind,error,closeError,...config};
 const selected=socket[name];
 const formats=[],sent=[],formatted=[];
 for(const m of ['send','sendWithDiagnostics'])socket[m]=tag(function(data){sent.push(data);},m);
 for(const m of ['emitWsDiagnosticSend','emitWsDiagnosticReceive'])socket[m]=tag(function(data,options){formats.push(options.format);},m);
 socket.subscribeUserTopics=tag(function(){},'subscribeUserTopics');
 const options=tag({vault:Object.hasOwn(config,'vault')?config.vault:'vault',applicationId:'application'},'options');
 if(config.freshUser){let count=0;Object.defineProperty(options,'vault',{get(){return 'vault-'+(++count)}});}
 const parent=socket.parent=tag({options,store:tag({markets:config.markets??[{id:'BTC',isSpot:false},{id:'dex:ABC',isSpot:false},{id:'PURR/USDC',isSpot:true}]},'store'),hip3DexNames:config.dexes??['dex'],shouldTrackSpotHoldings:tag(function(){return config.track??false;},'shouldTrackSpotHoldings'),shouldRequestPortfolioMarginSpotState:tag(function(){return config.portfolio??false;},'shouldRequestPortfolioMarginSpotState')},'parent');
 socket.topics=tag(config.topics??{'candle.BTC.1m':1,'l2Book.ETH':1,'ignored.BTC':1},'topics');
 globals.systemSubscriptionTypes=tag(classes[kind].types,'systemTypes');
 globals.Object=tag({keys:tag(Object.keys,'keys')},'Object');globals.flatten=tag(require('lodash/flatten'),'flatten');
 globals.JSON=tag({stringify:tag(JSON.stringify,'stringify')},'JSON');
 for(const n of ['formatHyperliquidWsDiagnostic','shouldEmitHyperliquidWsSendDiagnostic','shouldEmitHyperliquidWsReceiveDiagnostic'])globals[n]=tag(diagnostics.exports[n],n);
 const input=native(socket),inputArgs=args.map(native);let result,thrown;
 try{result=selected(...args);}catch(e){thrown=e;}
 const mainStop=calls.length;
 if(config.fireFormat&&formats.length){try{formatted.push({value:native(formats[0](config.formatMessage??'hello'))});}catch(e){formatted.push({throw:native(e)});}}
 cases.push({kind,name,label,baseline:Object.keys(config).length===0,input,args:inputArgs,calls,mainStop,result:thrown?{throw:native(thrown)}:{value:native(result)},fireFormat:!!config.fireFormat&&formats.length>0,formatMessage:native(config.formatMessage??'hello'),formatted,sent:sent.map(native),formatCount:formats.length});
}
const sendData=['raw','{"method":"ping"}','{"method":"subscribe","subscription":{"type":"activeAssetData"}}','{"method":"subscribe","subscription":{"type":"allMids"}}','0x'+'a'.repeat(40),'bad json',''];
const receiveJson=[{},null,{channel:'allMids'},{channel:'error'},{channel:'userFills'},{channel:'SubscriptionResponse'},{channel:'pong'}];
for(const kind of ['public','private']){
 for(const value of sendData)add(kind,'sendWithDiagnostics','send predicate',[value]);
 for(const config of [{fireFormat:true},{fireFormat:true,formatMessage:'0x'+'a'.repeat(40)},{fireFormat:true,throwGlobal:'formatHyperliquidWsDiagnostic'},{throwCall:'send'},{throwCall:'shouldEmitHyperliquidWsSendDiagnostic'},{throwCall:'emitWsDiagnosticSend'},{throwGet:'emitWsDiagnosticSend'}])add(kind,'sendWithDiagnostics','send failures and format',['raw'],config);
 for(const json of receiveJson)add(kind,'emitReceiveDiagnostics','receive predicate',['raw',json]);
 for(const config of [{fireFormat:true},{fireFormat:true,formatMessage:{coin:'BTC'}},{fireFormat:true,throwGlobal:'formatHyperliquidWsDiagnostic'},{throwCall:'shouldEmitHyperliquidWsReceiveDiagnostic'},{throwCall:'emitWsDiagnosticReceive'},{throwGet:'emitWsDiagnosticReceive'},{fireFormat:true,throwCall:'formatHyperliquidWsDiagnostic'}])add(kind,'emitReceiveDiagnostics','receive failures and format',['raw',{channel:'error'}],config);
}
for(const config of [{},{track:true},{track:true,portfolio:true},{track:true,portfolio:false},{vault:''},{dexes:[]},{dexes:['A','B']},{throwCall:'sendWithDiagnostics'},{throwCall:'shouldTrackSpotHoldings'},{track:true,throwCall:'shouldRequestPortfolioMarginSpotState'},{throwNext:1}])add('private','subscribeSystemTopics','core system',[],config);
for(const config of [{},{markets:[]},{markets:[{id:'BTC',isSpot:false}]},{markets:[{id:'dex:X',isSpot:false},{id:'PURR/USDC',isSpot:true}]},{markets:[null]},{markets:[{id:undefined,isSpot:true}]},{markets:[{id:undefined,isSpot:false}]},{vault:''},{throwCall:'sendWithDiagnostics'},{throwCall:'sendWithDiagnostics',throwClose:true},{throwNext:1}])add('private','subscribeAllActiveAssetData','active assets',[],config);
for(const config of [{},{throwCall:'send'},{throwGet:'send'},{throwGlobal:'JSON'},{throwCall:'subscribeUserTopics'}])add('spot_public','subscribePublicTopics','spot public',[],config);
for(const topics of [{},{'candle.BTC.1m':1},{'l2Book.ETH':1},{'candle':1},{'candle.BTC':1},{'candle.BTC.1m.extra':1},{'l2Book.':1},{'ignore.BTC':1}])add('spot_public','subscribeUserTopics','topic shape',[],{topics});
add('spot_public','subscribeUserTopics','topic baseline');
for(const config of [{throwCall:'send'},{throwCall:'send',throwClose:true},{throwCall:'flatten'},{throwNext:1}])add('spot_public','subscribeUserTopics','topic failures',[],config);
for(const config of [{},{vault:''},{vault:null},{vault:0},{freshUser:true},{throwCall:'send'},{throwCall:'send',throwClose:true},{throwGet:'vault'},{throwGlobal:'systemSubscriptionTypes'},{throwNext:1}])add('spot_private','subscribeSystemTopics','spot system',[],config);
for(const config of [{},{markets:[]},{markets:[{id:'@7'},{id:'dex:X'}]},{markets:[null]},{vault:''},{freshUser:true},{throwCall:'send'},{throwCall:'send',throwClose:true},{throwGet:'id'},{throwNext:1}])add('spot_private','subscribeCoins','spot coins',[],config);
function getterMarkets(){let step=0,reads=0;return{[Symbol.iterator](){return{next(){if(step++)return{done:true};return{done:false,get value(){return{id:++reads===1?'BTC':'ETH',isSpot:false};}};}};}};}
add('private','subscribeAllActiveAssetData','iterator single read',[],{markets:getterMarkets()});
add('spot_private','subscribeCoins','iterator single read',[],{markets:getterMarkets()});
function payloadFields(c){return c.calls.filter(e=>e.op==='call'&&e.args.payload&&e.args.function[2]==='stringify').map(e=>{const sub=e.args.payload[2].find(([k])=>String.fromCharCode(...k)==='subscription')[1];return Object.fromEntries(sub[2].map(([k,v])=>[String.fromCharCode(...k),v]));});}
const activeBaseline=cases.find(c=>c.kind==='private'&&c.name==='subscribeAllActiveAssetData'&&c.baseline);
assert.deepEqual(payloadFields(activeBaseline).map(p=>p.coin),[['string',units('BTC')]],'CORE_ACTIVE_BASELINE native BTC emission');
assert.equal(activeBaseline.sent.length,1,'CORE_ACTIVE_BASELINE one native sender argument');
const topicsBaseline=cases.find(c=>c.kind==='spot_public'&&c.label==='topic baseline');
assert.deepEqual(payloadFields(topicsBaseline).map(p=>p.type),[['string',units('candle')],['string',units('l2Book')]],'SPOT_TOPICS_BASELINE native candle/l2Book emission');
assert.equal(topicsBaseline.sent.length,2,'SPOT_TOPICS_BASELINE two native sender arguments');
for(const c of cases.filter(c=>c.label==='iterator single read'))assert.deepEqual(payloadFields(c).map(p=>p.coin),[['string',units('BTC')]],'ITERATOR_SOURCE_SINGLE_READ native BTC value '+c.kind);
assert.equal(cases.length,118,'independent56+23+18+21');
console.log(JSON.stringify({names:Object.entries(sourceNames).flatMap(([kind,names])=>names.map(name=>kind+'.'+name)),cases,iterationStop:117,scope:'ten emission algorithms, native literal payloads and source-order calls/iterator close/formatter callbacks; full classes/global/intrinsic/callee implementation and live sockets unproved'}));
