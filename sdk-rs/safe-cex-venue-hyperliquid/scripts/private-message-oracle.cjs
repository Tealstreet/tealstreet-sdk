'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../..');
const sourceNames={core:['isStaleMessage','onMessage','processMessage','handleFundings','handleUserNonFundingLedgerUpdates','handleUserEvents','handleServerError','handleHip3AllMids'],public:['handleAllMids','handleFastAssetCtxs']};
const names=Object.entries(sourceNames).flatMap(([k,ns])=>ns.map(n=>k+'.'+n));
let g,ids,tags,nextId,calls,protos,createdSets;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),tag=(v,t)=>(tags.set(v,t),v);
function native(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['bool',v];if(typeof v==='string')return['string',units(v)];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'nan':b.toString('hex')];}if(!ids.has(v))ids.set(v,nextId++);return['ref',ids.get(v),tags.get(v)||typeof v,typeof v];}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(error){e.throw=native(error);throw error;}}
function tree(v){if(tags.get(v)==='literal')return['literal',native(v),native(Object.getPrototypeOf(v)),Object.keys(v).map(k=>[units(k),tree(v[k])])];return native(v);}
const globals={};
const bind={
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish get');const v=Reflect.get(Object(object),name,object);if(typeof v==='function')tag(v,name);return v;}),
 __optionalGet:(object,name)=>object==null?undefined:bind.__get(object,name),
 __optionalIndex:(object,key)=>object==null?undefined:bind.__index(object,key),
 __index:(object,key)=>op('index',{object:native(object),key:native(key)},()=>{if(object==null)throw new TypeError('nullish index');return Reflect.get(Object(object),key,object);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;}),
 __setIndex:(object,key,value)=>op('setIndex',{object:native(object),key:native(key),value:native(value)},()=>{object[key]=value;}),
 __defineIndex:(object,key,value)=>op('defineIndex',{object:native(object),key:native(key),value:native(value)},()=>{Object.defineProperty(object,key,{value,writable:true,enumerable:true,configurable:true});}),
 __call:(fn,receiver,...args)=>{const frame={op:'call',args:{function:native(fn),receiver:native(receiver),args:args.map(native)}};calls.push(frame);try{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;
  if(tags.get(fn)==='filter'){frame.callbacks=[];const original=args[0];args[0]=function(entry){const invocation={input:native(entry)};frame.callbacks.push(invocation);try{const result=original(entry);invocation.result=native(result);return result;}catch(error){invocation.throw=native(error);throw error;}};}
  const result=Reflect.apply(fn,receiver,args);frame.result=native(result);return result;
 }catch(error){frame.throw=native(error);throw error;}},
 __fillPredicate:fn=>op('predicate',{},()=>tag(fn,'validFillPredicate')),
 __has:(name,object)=>op('has',{name,object:native(object)},()=>name in object),
 __construct:(fn,...args)=>op('construct',{function:native(fn),args:args.map(native)},()=>{if(g.throwConstruct)throw g.error;const value=tag(Reflect.construct(fn,args),'updatedSymbols');createdSets.push(value);return value;}),
 __regex:value=>op('regex',{pattern:value.toString()},()=>tag(value,'subscriptionRegex')),
 __forIn:object=>op('forIn',{object:native(object)},()=>{const generator=(function*(){for(const key in object)yield key;})();const enumeration=tag({[Symbol.iterator](){return this;},next(){const frame={op:'forInNext',args:{iterator:native(enumeration)}};calls.push(frame);try{const step=generator.next();frame.result=step.done?['done']:native(step.value);return step;}catch(error){frame.throw=native(error);throw error;}}},'enumeration');return enumeration;}),
 __newObject:()=>{const value=op('object',{},()=>tag({},'literal'));calls.at(-1).prototype=native(Object.getPrototypeOf(value));return value;},
 __newArray:()=>op('array',{},()=>tag([],'array')),
 __arrayDefine:(object,index,value)=>op('arrayDefine',{object:native(object),index,value:native(value)},()=>{Object.defineProperty(object,index,{value,writable:true,enumerable:true,configurable:true});}),
 __define:(object,name,value)=>op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});}),
 __spread:(object,value)=>{const e={op:'spread',args:{object:native(object),source:native(value),members:[]}};calls.push(e);try{if(value!=null)for(const key of Reflect.ownKeys(Object(value))){const d=Object.getOwnPropertyDescriptor(Object(value),key);if(d?.enumerable){const member=value[key];e.args.members.push([key,native(member)]);Object.defineProperty(object,key,{value:member,writable:true,enumerable:true,configurable:true});}}e.result=native(undefined);}catch(error){e.throw=native(error);throw error;}},
 __predicate:(upper,tokenIndex,fn)=>op('predicate',{upper:native(upper),tokenIndex:native(tokenIndex)},()=>tag(fn,'spotPredicate')),
 __pair:value=>{const e={op:'pair',args:{value:native(value)}};calls.push(e);try{const[a,b]=value;e.result=[native(a),native(b)];return[a,b];}catch(error){e.throw=native(error);throw error;}},
 __template:value=>op('template',{value:native(value)},()=>`${value}`),
 __operator:(operator,left,right)=>op('operator',{operator,left:native(left),right:native(right)},()=>{switch(operator){case'subtract':return left-right;case'divide':return left/right;case'less':return left<right;case'lessEqual':return left<=right;case'greater':return left>right;default:throw new Error(operator);}}),
 __iterate:value=>op('iterator',{value:native(value)},()=>{
  const real=value[Symbol.iterator]();if(real===null||(typeof real!=='object'&&typeof real!=='function'))throw new TypeError('iterator object');const nextMethod=real.next;let count=0;
  const iterator=tag({[Symbol.iterator](){return this;},next(){const e={op:'next',args:{iterator:native(iterator)}};calls.push(e);try{if(++count===g.throwNext)throw g.error;const step=Reflect.apply(nextMethod,real,[]);if(step===null||(typeof step!=='object'&&typeof step!=='function'))throw new TypeError('iterator result object');const done=!!step.done,value=done?undefined:step.value;e.result=done?['done']:native(value);return{done,value};}catch(error){e.throw=native(error);throw error;}},return(){return op('close',{iterator:native(iterator)},()=>{if(g.throwClose)throw g.closeError;const close=real.return;if(close==null)return{};const result=Reflect.apply(close,real,[]);if(result===null||(typeof result!=='object'&&typeof result!=='function'))throw new TypeError('iterator return object');return result;});}},'iterator');return iterator;
 }),
};
const globalNames=new Set(['Math','parseFloat','OrderSide','performance','jsonParse','HyperliquidPrivateWebSocket','Array','Boolean','LogSeverity','Number','Set','decodeHyperliquidFastAssetCtxs']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args),variable=(name,value)=>f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(name,undefined,undefined,value)],ts.NodeFlags.Const));
 const visit=n=>{
  if(ts.isForInStatement(n))return f.createForOfStatement(undefined,n.initializer,call('__forIn',[ts.visitNode(n.expression,visit)]),ts.visitNode(n.statement,visit));
  if(ts.isTypeNode(n))return n;
  if(ts.isForOfStatement(n)){
   const expression=call('__iterate',[ts.visitNode(n.expression,visit)]),body=ts.visitNode(n.statement,visit);
   if(ts.isVariableDeclarationList(n.initializer)&&n.initializer.declarations.length===1&&ts.isArrayBindingPattern(n.initializer.declarations[0].name)){
    const declaration=n.initializer.declarations[0];assert.equal(declaration.name.elements.length,2,'tuple binding floor');
    const raw=f.createIdentifier('__tupleValue'),initializer=f.createVariableDeclarationList([f.createVariableDeclaration(raw)],ts.NodeFlags.Const);
    const binding=variable(declaration.name,call('__pair',[raw]));
    return f.updateForOfStatement(n,n.awaitModifier,initializer,expression,f.createBlock([binding,...(ts.isBlock(body)?body.statements:[body])],true));
   }
   return f.updateForOfStatement(n,n.awaitModifier,n.initializer,expression,body);
  }
  if(ts.isVariableDeclaration(n)&&ts.isArrayBindingPattern(n.name))return f.updateVariableDeclaration(n,n.name,n.exclamationToken,n.type,call('__pair',[ts.visitNode(n.initializer,visit)]));
  if(ts.isRegularExpressionLiteral(n))return call('__regex',[n]);
  if(ts.isTemplateExpression(n))return f.updateTemplateExpression(n,n.head,n.templateSpans.map(s=>f.updateTemplateSpan(s,call('__template',[ts.visitNode(s.expression,visit)]),s.literal)));
  if(ts.isBinaryExpression(n)){const kind=n.operatorToken.kind;if(kind===ts.SyntaxKind.InKeyword)return call('__has',[ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);if(kind===ts.SyntaxKind.EqualsToken&&ts.isPropertyAccessExpression(n.left))return call('__assign',[ts.visitNode(n.left.expression,visit),f.createStringLiteral(n.left.name.text),ts.visitNode(n.right,visit)]);if(kind===ts.SyntaxKind.EqualsToken&&ts.isElementAccessExpression(n.left))return call('__setIndex',[ts.visitNode(n.left.expression,visit),ts.visitNode(n.left.argumentExpression,visit),ts.visitNode(n.right,visit)]);const ops=new Map([[ts.SyntaxKind.MinusToken,'subtract'],[ts.SyntaxKind.SlashToken,'divide'],[ts.SyntaxKind.LessThanToken,'less'],[ts.SyntaxKind.LessThanEqualsToken,'lessEqual'],[ts.SyntaxKind.GreaterThanToken,'greater']]);if(ops.has(kind)){const op=call('__operator',[f.createStringLiteral(ops.get(kind)),ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);return kind===ts.SyntaxKind.PlusEqualsToken?f.createBinaryExpression(n.left,f.createToken(ts.SyntaxKind.EqualsToken),op):op;}}
  if(ts.isNewExpression(n))return call('__construct',[ts.visitNode(n.expression,visit),...(n.arguments??[]).map(a=>ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&globalNames.has(n.expression.text))return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...n.arguments.map(a=>ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
   const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
   const statements=[variable(recv,ts.visitNode(n.expression.expression,visit))];
   if(n.expression.questionDotToken)statements.push(f.createIfStatement(f.createBinaryExpression(recv,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createReturnStatement(f.createIdentifier('undefined'))));
   const args=n.arguments.map(a=>ts.isArrowFunction(a)?call('__fillPredicate',[ts.visitNode(a,visit)]):ts.visitNode(a,visit));
   statements.push(variable(fn,call('__get',[recv,f.createStringLiteral(n.expression.name.text)])),f.createReturnStatement(call('__call',[fn,recv,...args])));
   return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock(statements,true))),undefined,[]);
  }
  if(ts.isPropertyAccessExpression(n)||ts.isElementAccessExpression(n)){
   const property=ts.isPropertyAccessExpression(n),key=property?f.createStringLiteral(n.name.text):ts.visitNode(n.argumentExpression,visit);
   if(ts.isPropertyAccessExpression(n.expression)&&n.expression.questionDotToken){const receiver=f.createIdentifier('__optionalReceiver');const inner=call('__get',[receiver,f.createStringLiteral(n.expression.name.text)]);const tail=call(property?(n.questionDotToken?'__optionalGet':'__get'):(n.questionDotToken?'__optionalIndex':'__index'),[inner,key]);const ret=f.createConditionalExpression(f.createBinaryExpression(receiver,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createToken(ts.SyntaxKind.QuestionToken),f.createIdentifier('undefined'),f.createToken(ts.SyntaxKind.ColonToken),tail);return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([variable(receiver,ts.visitNode(n.expression.expression,visit)),f.createReturnStatement(ret)],true))),undefined,[]);}
   const get=call(property?'__get':'__index',[ts.visitNode(n.expression,visit),key]);
   if(n.questionDotToken){const receiver=f.createIdentifier('__optionalReceiver');const ret=f.createConditionalExpression(f.createBinaryExpression(receiver,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createToken(ts.SyntaxKind.QuestionToken),f.createIdentifier('undefined'),f.createToken(ts.SyntaxKind.ColonToken),call(property?'__get':'__index',[receiver,key]));return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([variable(receiver,ts.visitNode(n.expression,visit)),f.createReturnStatement(ret)],true))),undefined,[]);}return get;
  }
  if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
  if(ts.isArrayLiteralExpression(n)){const array=f.createIdentifier('__array');const statements=[variable(array,call('__newArray',[]))];n.elements.forEach((v,i)=>statements.push(f.createExpressionStatement(call('__arrayDefine',[array,f.createNumericLiteral(i),ts.visitNode(v,visit)]))));return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(array)],true))),undefined,[]);}
  if(ts.isObjectLiteralExpression(n)){const object=f.createIdentifier('__literal');const statements=[variable(object,call('__newObject',[]))];for(const p of n.properties){if(ts.isSpreadAssignment(p)){statements.push(f.createExpressionStatement(call('__spread',[object,ts.visitNode(p.expression,visit)])));continue;}assert(ts.isShorthandPropertyAssignment(p)||ts.isPropertyAssignment(p));const value=ts.isShorthandPropertyAssignment(p)?ts.visitNode(p.name,visit):ts.visitNode(p.initializer,visit);const computed=ts.isComputedPropertyName(p.name);const key=computed?ts.visitNode(p.name.expression,visit):f.createStringLiteral(p.name.getText());if(computed){const k=f.createIdentifier('__computedKey'+statements.length);statements.push(variable(k,key),f.createExpressionStatement(call('__defineIndex',[object,k,value])));}else statements.push(f.createExpressionStatement(call('__define',[object,key,value])));}return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(object)],true))),undefined,[]);}
  return ts.visitEachChild(n,visit,context);
 };return s=>ts.visitNode(s,visit);
}
const classes={};
for(const [kind,methods] of Object.entries(sourceNames)){
 const file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-${kind==='public'?'public':'private'}.ts`),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
 const code=methods.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const t=ts.transform(m.initializer??m.body,[transformer]);let printed=printer.printNode(m.initializer?ts.EmitHint.Expression:ts.EmitHint.Unspecified,t.transformed[0],ast);t.dispose();
  if(name==='onMessage'){// Account for the original observable parameter binding outside its try.
   printed=printed.replace(/\(\{ data \}[^)]*\) => \{/,"(__event) => { const data=__get(__event,'data');");assert(printed.includes("const data=__get"),'destructure source binding');
  }
  return m.initializer?`${name}=${printed};`:`${name}(${m.parameters.map(p=>p.name.getText(ast)).join(',')})${printed}`;
 }).join('\n');
 const skip=kind==='core'?cls.members.find(m=>m.name?.getText(ast)==='SKIP_TIMESTAMP_CHECK_CHANNELS').getText(ast):'';
 const context={...bind,exports:{}};vm.runInNewContext(ts.transpileModule(`class ${cls.name.text}{${skip}\n${code}}exports.Class=${cls.name.text};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);classes[kind]=context.exports.Class;
}
const cases=[];
const effectNames=['trackLastReceived','emitReceiveDiagnostics','recordMessageHandlerStats','processMessage','isStaleMessage','handleHip3AllMids','handleServerError','handleSpotClearinghouseStateUpdate','handleHip3ClearinghouseState','applyCoreClearinghouseState','handleFills','handleFundings','handleUserNonFundingLedgerUpdates','handleOrderUpdates','handleUserEvents','handleSpotClearinghouseState','handlePongEvent','updatePosition','update','applySpotBalances','emit','requestSpotBalanceRefresh','requestUnifiedSpotCollateralRefresh','log','updateTicker','syncBalanceFromLivePositions'];
function add(kind,name,label,arg,config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];createdSets=[];g={error:tag({label},'error'),...config};tag(Object.prototype,'defaultPrototype');
 const socket=tag(new classes[kind](),'socket'),effects=[];
 const selected=socket[name];
 function virtual(target,n,result){target[n]=tag(function(...args){effects.push({name:n,receiver:native(this),args:args.map(v=>tags.get(v)?.endsWith('UpdateBuf')?['buffer',native(v),Object.keys(v).map(k=>[units(k),native(v[k])])]:tree(v))});return result;},n);}
 for(const n of effectNames)virtual(socket,n,n==='isStaleMessage'?(config.stale??false):undefined);
 socket.isDisposed=config.disposed??false;socket.pingAt=Object.hasOwn(config,'pingAt')?config.pingAt:17.5;
 const positions=tag(config.positions??[{symbol:'OTHER',contracts:9},{symbol:'BTC/USD',contracts:4,marginType:'isolated',leverage:17.5},{symbol:'BTC/USD',contracts:0,marginType:'cross',leverage:91.25}],'positions');
 const markets=tag(Object.assign(Object.create(null),config.markets??{BTC:{symbol:'BTC/USD'}}),'marketsById');
 const store=tag({marketsById:markets,positions},'store');virtual(store,'updatePosition');virtual(store,'update');virtual(store,'updateTicker');socket.store=store;store.tickersById=tag(Object.assign(Object.create(null),config.tickers??{BTC:{symbol:'BTC/USD'},'xyz:BTC':{symbol:'XYZ-BTC/USD'}}),'tickersById');
 const emitter=tag({},'emitter');virtual(emitter,'emit');
 const parent=tag({store,emitter,activeAssetData:tag(new Map(),'activeAssetData')},'parent');socket.parent=parent;parent.options=tag({vault:config.vault??'vault-user',applicationId:'application-user'},'options');
 for(const[n,v]of Object.entries({_hip3MidsUpdateBuf:{last:0,midPrice:0},_midsUpdateBuf:{last:0,midPrice:0},_fastMarkUpdateBuf:{mark:0},_fastMidUpdateBuf:{last:0,midPrice:0},_fastMarkAndMidUpdateBuf:{mark:0,last:0,midPrice:0}}))socket[n]=tag(v,n);

 for(const n of ['applySpotBalances','requestSpotBalanceRefresh','requestUnifiedSpotCollateralRefresh','handleSpotClearinghouseStateUpdate','log','syncBalanceFromLivePositions'])virtual(parent,n);
 parent.shouldTrackSpotHoldings=tag(function(){return config.track??true;},'shouldTrackSpotHoldings');
 parent._getSymbolConfigurationFromAssetData=tag(function(){return tag({positionMode:'hedged',marginType:'cross',leverage:33.5},'symbolConfiguration');},'_getSymbolConfigurationFromAssetData');
 const lastTime=tag(new Map(config.times??[]),'lastTime'),lastDataHash=tag(new Map(config.hashes??[]),'lastDataHash');socket.timestampState=tag({lastTime,lastDataHash},'timestampState');
 socket.getMessageTimestamp=tag(function(){return Object.hasOwn(config,'time')?config.time:null;},'getMessageTimestamp');socket.getStaleCheckChannelKey=tag(function(channel,data){return data.dex?channel+':'+data.dex:channel;},'getStaleCheckChannelKey');
 const parsed=Object.hasOwn(config,'parsed')?config.parsed:arg;
 globals.jsonParse=tag(function(){if(config.parseThrows)throw g.error;return parsed;},'jsonParse');
 globals.performance=tag({now:tag(function(){return config.now??91.25;},'now')},'performance');globals.Math=tag(Math,'Math');globals.parseFloat=tag(parseFloat,'parseFloat');
 globals.Array=tag(Array,'Array');globals.Boolean=tag(Boolean,'Boolean');globals.Number=tag(Number,'Number');globals.Set=tag(Set,'Set');globals.LogSeverity=tag({Warning:'warning'},'LogSeverity');globals.decodeHyperliquidFastAssetCtxs=tag(function(){return config.decoded??{};},'decodeHyperliquidFastAssetCtxs');
 globals.OrderSide=tag({Buy:'buy',Sell:'sell'},'OrderSide');globals.HyperliquidPrivateWebSocket=tag(classes.core,'HyperliquidPrivateWebSocket');
 const maps=[lastTime,lastDataHash,parent.activeAssetData],mapSeed=maps.map(m=>({value:native(m),entries:[...m].map(([k,v])=>[native(k),native(v)])}));
 let inputArg=name==='onMessage'?tag({data:'raw frame'},'event'):arg;
 if(typeof inputArg==='object'&&inputArg!==null&&!tags.has(inputArg))tag(inputArg,'input');
 let coinReads=0;if(config.freshCoin&&parsed?.data)Object.defineProperty(parsed.data,'coin',{get(){return ++coinReads<=2?'BTC':'OTHER';},configurable:true});
 const buffers=['_hip3MidsUpdateBuf','_midsUpdateBuf','_fastMarkUpdateBuf','_fastMidUpdateBuf','_fastMarkAndMidUpdateBuf'].map(n=>({value:native(socket[n]),fields:Object.keys(socket[n]).map(k=>[units(k),native(socket[n][k])])}));
 const input=native(socket),args=[native(inputArg)];let result;try{result={value:tree(Reflect.apply(selected,socket,[inputArg]))};}catch(error){result={throw:native(error)};}
 const mapFinal=maps.map(m=>({value:native(m),entries:[...m].map(([k,v])=>[native(k),native(v)])}));
 cases.push({kind,name,label,input,args,calls,mapSeed,mapFinal,buffers,bufferFinal:buffers.map(b=>({value:b.value,fields:Object.keys(socket[b.value[2]]).map(k=>[units(k),native(socket[b.value[2]][k])])})),sets:createdSets.map(set=>({value:native(set),values:[...set].map(native)})),effects,result});
}
// Fixed lists are authored independently of emitted cases; these are floors.
const staleCases=[
 ['timestamp new',{channel:'clearinghouseState',data:{}},{time:150,times:[['clearinghouseState',138]]}],
 ['timestamp equal',{channel:'clearinghouseState',data:{}},{time:138,times:[['clearinghouseState',138]]}],
 ['timestamp older',{channel:'clearinghouseState',data:{}},{time:12,times:[['clearinghouseState',138]]}],
 ['missing channel',{data:{}},{}],['missing data',{channel:'activeAssetData'},{}],['undefined time',{channel:'allMids',data:{}},{time:undefined}],
 ['zero time',{channel:'allMids',data:{}},{time:0}],['nan time',{channel:'allMids',data:{}},{time:NaN}],
 ['hash new',{channel:'activeAssetData',data:{coin:'BTC',markPx:'91.25',leverage:{value:17.5}}},{}],
 ['hash same',{channel:'activeAssetData',data:{coin:'BTC',markPx:'91.25',leverage:{value:17.5}}},{hashes:[['activeAssetData:BTC','91.25:17.5']]}],
 ['hash changed',{channel:'activeAssetData',data:{coin:'BTC',markPx:'91.25',leverage:{value:33.5}}},{hashes:[['activeAssetData:BTC','91.25:17.5']]}],
 ['hash falsy',{channel:'activeAssetData',data:{coin:'BTC',markPx:0,leverage:{value:0}}},{}],
 ['coin empty',{channel:'activeAssetData',data:{coin:''}},{}],['other channel',{channel:'unknown',data:{}},{}],['null input',null,{}],
 ['timestamp lookup error',{channel:'allMids',data:{}},{throwCall:'getMessageTimestamp'}],['map set error',{channel:'allMids',data:{}},{time:150,throwCall:'set'}],
 ['hash getter error',{channel:'activeAssetData',data:{coin:'BTC'}},{throwGet:'markPx'}],
 ['dex timestamp',{channel:'clearinghouseState',data:{dex:'xyz'}},{time:150,times:[['clearinghouseState:xyz',138]]}],
 ['timestamp getter error',{channel:'allMids',data:{}},{throwGet:'timestampState'}],
];assert.equal(staleCases.length,20);for(const [l,v,c]of staleCases)add('core','isStaleMessage',l,v,c);
const coreMessageCases=[
 ['pong latency',{channel:'pong'},{}],['pong no clock',{channel:'pong'},{pingAt:0}],['disposed',{channel:'allMids'},{disposed:true}],
 ['tracking error',{channel:'allMids'},{throwCall:'trackLastReceived'}],['parameter error',{channel:'allMids'},{throwGet:'data'}],
 ['disposed getter error',{channel:'allMids'},{throwGet:'isDisposed'}],['parse error',null,{parseThrows:true}],['null parse',null,{}],
 ['diagnostic error',{channel:'allMids'},{throwCall:'emitReceiveDiagnostics'}],['stale',{channel:'allMids'},{stale:true}],
 ['fresh',{channel:'allMids'},{stale:false}],['skip fills',{channel:'userFills'},{}],['skip orders',{channel:'orderUpdates'},{}],
 ['subscription lower',{channel:'subscriptionResponse'},{}],['subscription capital',{channel:'SubscriptionResponse'},{}],['subscribed',{channel:'subscribed'},{}],
 ['missing channel',{},{}],['empty channel',{channel:''},{}],['stats error',{channel:'pong'},{throwCall:'recordMessageHandlerStats'}],
 ['clock error',{channel:'pong'},{throwCall:'now'}],['latency error',{channel:'pong'},{throwCall:'update'}],['dispatch error',{channel:'allMids'},{throwCall:'processMessage'}],
 ['skip lookup error',{channel:'allMids'},{throwCall:'has'}],['stale error',{channel:'allMids'},{throwCall:'isStaleMessage'}],
];assert.equal(coreMessageCases.length,24);for(const[l,v,c]of coreMessageCases)add('core','onMessage',l,v,c);
const processCases=[
 ['active position fields',{channel:'activeAssetData',data:{coin:'BTC'}},{}],['active no market',{channel:'activeAssetData',data:{coin:'MISSING'}},{}],
 ['active no coin',{channel:'activeAssetData',data:{}},{}],['active missing data',{channel:'activeAssetData'},{}],['active coin reads',{channel:'activeAssetData',data:{coin:'BTC'}},{freshCoin:true}],
 ['mid fallback',{channel:'allMids'},{}],['server error',{channel:'error',data:'private diagnostic'},{}],
 ['spot nested',{channel:'spotState',data:{spotState:{balances:[1]}}},{}],['spot raw',{channel:'spotClearinghouseState',data:{balances:[2]}},{}],['spot fallback',{channel:'spotState'},{}],
 ['core nested',{channel:'clearinghouseState',data:{dex:'',clearinghouseState:{assetPositions:[1]}}},{}],['hip3 nested',{channel:'clearinghouseState',data:{dex:'xyz',clearinghouseState:{assetPositions:[2]}}},{}],
 ['core unwrapped',{channel:'clearinghouseState',data:{dex:''}},{}],['core absent',{channel:'clearinghouseState'},{}],
 ['fills fallback',{channel:'userFills'},{}],['fundings fallback',{channel:'userFundings'},{}],['ledger fallback',{channel:'userNonFundingLedgerUpdates'},{}],['orders fallback',{channel:'orderUpdates'},{}],
 ['events absent',{channel:'userEvents'},{}],['unknown channel',{channel:'unknown'},{}],['null json',null,{}],
 ['route error',{channel:'userFills'},{throwCall:'handleFills'}],['position update error',{channel:'activeAssetData',data:{coin:'BTC'}},{throwCall:'updatePosition'}],
 ['position symbol error',{channel:'activeAssetData',data:{coin:'BTC'}},{throwGet:'symbol'}],
];assert.equal(processCases.length,24);for(const[l,v,c]of processCases)add('core','processMessage',l,v,c);

for(const[l,v]of [['fundings opaque',{}],['fundings null',null],['fundings undefined',undefined]])add('core','handleFundings',l,v);
for(const [l,c]of [['refresh tracked',{}],['no holdings',{track:false}],['truthy holdings',{track:'yes'}],['null holdings',{track:null}],['tracking error',{throwCall:'shouldTrackSpotHoldings'}],['refresh error',{throwCall:'requestUnifiedSpotCollateralRefresh'}],['parent getter error',{throwGet:'parent'}],['refresh getter error',{throwGet:'requestUnifiedSpotCollateralRefresh'}]])add('core','handleUserNonFundingLedgerUpdates',l,{},c);
const valid={tid:150,coin:'BTC',px:'91.25',sz:'4.75'};
const eventCases=[
 ['null event',null,{}],['undefined event',undefined,{}],['empty array',[],{}],['valid fill array',[valid],{}],['mixed array',[null,0,'text',{},valid],{}],
 ['orders array',[{order:{coin:'BTC'}}],{}],['object fills',{fills:[valid],isSnapshot:true,user:'message-user'},{}],['object orders',{orders:[{order:{coin:'BTC'}}]},{}],
 ['both fields',{fills:[valid],orders:[{order:{coin:'BTC'}}]},{}],['invalid fill fields',{fills:[{tid:1,coin:'BTC',px:0}]},{}],['no user',{fills:[valid]},{vault:''}],
 ['predicate error',{fills:[valid]},{throwGet:'filter'}],['fills error',{fills:[valid]},{throwCall:'handleFills'}],['orders error',{orders:[{}]},{throwCall:'handleOrderUpdates'}],
 ['inherited valid fields',[Object.create(valid)],{}],['falsy predicates',[null,undefined,false,0,''],{}],
];assert.equal(eventCases.length,16);for(const[l,v,c]of eventCases)add('core','handleUserEvents',l,v,c);
const serverCases=[['typed error','bad {"type":"userFills","user":"secret"}',{}],['no type','wallet-address',{}],['empty type','{"type":""}',{}],['null error',null,{}],['boxed string',new String('{"type":"x"}'),{}],['regex whitespace','{"type" : "allMids"}',{}],['log failure','{"type":"userFills"}',{throwCall:'log'}],['match failure','{"type":"userFills"}',{throwCall:'match'}]];
assert.equal(serverCases.length,8);for(const[l,v,c]of serverCases)add('core','handleServerError',l,v,c);
const midsCases=[
 ['known mids',{'xyz:BTC':'91.25',BTC:'17.5',unknown:'150'},{}],['empty mids',{},{}],['zero negative',{'xyz:BTC':'0',BTC:'-12'},{}],
 ['nonfinite mids',{'xyz:BTC':'NaN',BTC:'Infinity'},{}],['parse tail',{'xyz:BTC':'91.25tail',BTC:'17.5tail'},{}],
 ['no positions',{'xyz:BTC':'91.25',BTC:'17.5'},{positions:[]}],['inherited mids',Object.assign(Object.create({'xyz:BTC':'91.25'}),{BTC:'17.5'}),{}],
 ['ticker update error',{'xyz:BTC':'91.25',BTC:'17.5'},{throwCall:'updateTicker'}],['buffer assignment error',{'xyz:BTC':'91.25',BTC:'17.5'},{throwSet:'midPrice'}],
 ['set allocation error',{'xyz:BTC':'91.25',BTC:'17.5'},{throwConstruct:true}],['sync error',{'xyz:BTC':'91.25',BTC:'17.5'},{throwCall:'syncBalanceFromLivePositions'}],
];assert.equal(midsCases.length,11);for(const[l,v,c]of midsCases){add('core','handleHip3AllMids',l,{mids:v},c);add('public','handleAllMids',l,v,c);}
for(const[l,v]of [['null mids',null],['string mids','raw'],['absent mids',undefined]])add('core','handleHip3AllMids',l,{mids:v});
const fastCases=[
 ['mark mid',{BTC:{markPx:'91.25',midPx:'17.5'}},{}],['mark only',{BTC:{markPx:'91.25',midPx:'NaN'}},{}],['mid only',{BTC:{markPx:0,midPx:'17.5'}},{}],
 ['neither',{BTC:{markPx:0,midPx:-12}},{}],['infinities',{BTC:{markPx:Infinity,midPx:NaN}},{}],['missing context',{BTC:null},{}],['unknown ticker',{UNKNOWN:{markPx:91.25,midPx:17.5}},{}],
 ['multiple',{BTC:{markPx:91.25,midPx:17.5},'xyz:BTC':{markPx:33.5,midPx:12}},{}],['no positions',{BTC:{markPx:91.25,midPx:17.5}},{positions:[]}],
 ['decoder error',{}, {throwCall:'decodeHyperliquidFastAssetCtxs'}],['update error',{BTC:{markPx:91.25,midPx:17.5}},{throwCall:'updateTicker'}],['empty decode',{},{}],
];assert.equal(fastCases.length,12);for(const[l,v,c]of fastCases)add('public','handleFastAssetCtxs',l,'compressed payload',{...c,decoded:v});
const expectation={'core.isStaleMessage':20,'core.onMessage':24,'core.processMessage':24,'core.handleFundings':3,'core.handleUserNonFundingLedgerUpdates':8,'core.handleUserEvents':16,'core.handleServerError':8,'core.handleHip3AllMids':14,'public.handleAllMids':11,'public.handleFastAssetCtxs':12};
assert.equal(cases.length,140,'independent20+24+24+3+8+16+8+14+11+12');for(const[n,count]of Object.entries(expectation))assert.equal(cases.filter(c=>c.kind+'.'+c.name===n).length,count,n);
console.log(JSON.stringify({names,effectNames,expectation,cases,iterationStop:139,scope:'ten perp core private/public dispatch roots; JS primitives/importeddecoder/virtualcallees fixture contracts; ownedrouting/indexloops/predicates/buffers/mapset/payloads/catch boundaries; full classes/live sockets outsideproof'}));
