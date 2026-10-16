'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../..');
const sourceNames={core:['getBuilderInfo','getOrderErrorTelemetry','postOrderAction','validateResponseAndExtractOrderIds','signActionOrDefer','scheduleCancel','setLeverage','changeMarginType'],spot:['setLeverage','changeMarginType']};
const names=['core.getBuilderInfo','core.getOrderErrorTelemetry','core.postOrderAction','core.validateResponseAndExtractOrderIds','core.signActionOrDefer','core.scheduleCancel','core.setLeverage','core.changeMarginType','spot.setLeverage','spot.changeMarginType'];
let g,ids,tags,nextId,calls,protos;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),tag=(v,t)=>(tags.set(v,t),v);
function native(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['bool',v];if(typeof v==='string')return['string',units(v)];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'nan':b.toString('hex')];}if(!ids.has(v))ids.set(v,nextId++);return['ref',ids.get(v),tags.get(v)||typeof v,typeof v];}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(error){e.throw=native(error);throw error;}}
function tree(v){if(tags.get(v)==='literal')return['literal',native(v),native(Object.getPrototypeOf(v)),Object.keys(v).map(k=>[units(k),tree(v[k])])];return native(v);}
const globals={};
const bind={
 __await:async value=>{const e={op:'await',args:{value:native(value)}};calls.push(e);try{const result=await value;e.result=native(result);e.state=g.state();return result;}catch(error){e.throw=native(error);e.state=g.state();throw error;}},
 __unary:value=>op('unary',{value:native(value)},()=>+value),
 __compare:(operator,left,right)=>op('compare',{operator,left:native(left),right:native(right)},()=>operator==='less'?left<right:left>=right),
 __construct:(constructor,...args)=>op('construct',{constructor:native(constructor),args:args.map(native)},()=>tag(Reflect.construct(constructor,args),'constructedError')),
 __instanceof:(value,constructor)=>op('instanceof',{value:native(value),constructor:native(constructor)},()=>{if(g.throwInstanceof)throw g.error;return value instanceof constructor;}),
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish get');const v=Reflect.get(Object(object),name,object);if(typeof v==='function')tag(v,name);return v;}),
 __optionalGet:(object,name)=>object==null?undefined:bind.__get(object,name),
 __optionalIndex:(object,key)=>object==null?undefined:bind.__index(object,key),
 __index:(object,key)=>op('index',{object:native(object),key:native(key)},()=>{if(object==null)throw new TypeError('nullish index');return Reflect.get(Object(object),key,object);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;}),
 __setIndex:(object,key,value)=>op('setIndex',{object:native(object),key:native(key),value:native(value)},()=>{object[key]=value;}),
 __defineIndex:(object,key,value)=>op('defineIndex',{object:native(object),key:native(key),value:native(value)},()=>{Object.defineProperty(object,key,{value,writable:true,enumerable:true,configurable:true});}),
 __call:(fn,receiver,...args)=>{const input={function:native(fn),receiver:native(receiver),args:args.map(native)};if(['post','signActionOrDefer','signL1Action'].includes(tags.get(fn)))input.payloads=args.map(tree);return op('call',input,()=>{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;calls.at(-1).invoked=true;return Reflect.apply(fn,receiver,args);});},
 __newObject:()=>{const value=op('object',{},()=>tag({},'literal'));calls.at(-1).prototype=native(Object.getPrototypeOf(value));return value;},
 __newArray:()=>op('array',{},()=>tag([],'array')),
 __arrayDefine:(object,index,value)=>op('arrayDefine',{object:native(object),index,value:native(value)},()=>{Object.defineProperty(object,index,{value,writable:true,enumerable:true,configurable:true});}),
 __define:(object,name,value)=>op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});}),
 __spread:(object,value)=>{const e={op:'spread',args:{object:native(object),source:native(value),members:[]}};calls.push(e);try{if(value!=null)for(const key of Reflect.ownKeys(Object(value))){const d=Object.getOwnPropertyDescriptor(Object(value),key);if(d?.enumerable){const member=value[key];e.args.members.push([key,native(member)]);Object.defineProperty(object,key,{value:member,writable:true,enumerable:true,configurable:true});}}e.result=native(undefined);}catch(error){e.throw=native(error);throw error;}},
 __predicate:(upper,tokenIndex,fn)=>op('predicate',{upper:native(upper),tokenIndex:native(tokenIndex)},()=>tag(fn,'spotPredicate')),
 __pair:value=>{const e={op:'pair',args:{value:native(value)}};calls.push(e);try{const[a,b]=value;e.result=[native(a),native(b)];return[a,b];}catch(error){e.throw=native(error);throw error;}},
 __template:value=>op('template',{value:native(value)},()=>`${value}`),
 __operator:(operator,left,right)=>op('operator',{operator,left:native(left),right:native(right)},()=>{switch(operator){case'add':return left+right;case'subtract':return left-right;case'multiply':return left*right;case'greater':return left>right;default:throw new Error(operator);}}),
 __iterate:value=>op('iterator',{value:native(value)},()=>{
  const real=value[Symbol.iterator]();if(real===null||(typeof real!=='object'&&typeof real!=='function'))throw new TypeError('iterator object');const nextMethod=real.next;let count=0;
  const iterator=tag({[Symbol.iterator](){return this;},next(){const e={op:'next',args:{iterator:native(iterator)}};calls.push(e);try{if(++count===g.throwNext)throw g.error;const step=Reflect.apply(nextMethod,real,[]);if(step===null||(typeof step!=='object'&&typeof step!=='function'))throw new TypeError('iterator result object');const done=!!step.done,value=done?undefined:step.value;e.result=done?['done']:native(value);return{done,value};}catch(error){e.throw=native(error);throw error;}},return(){return op('close',{iterator:native(iterator)},()=>{if(g.throwClose)throw g.closeError;const close=real.return;if(close==null)return{};const result=Reflect.apply(close,real,[]);if(result===null||(typeof result!=='object'&&typeof result!=='function'))throw new TypeError('iterator return object');return result;});}},'iterator');return iterator;
 }),
};
const globalNames=new Set(['Number','Array','Boolean','Error','ENDPOINTS','virtualClock','resolveHyperliquidBuilder','parseHyperliquidExchangePayload','maskHyperliquidAddress','parseHyperliquidOrderResponse','HyperliquidUnreadableOrderResponseError','signL1Action','createWalletFromPrivateKey']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args),variable=(name,value)=>f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(name,undefined,undefined,value)],ts.NodeFlags.Const));
 const visit=n=>{
  if(ts.isTypeNode(n))return n;
  if(ts.isAwaitExpression(n))return f.updateAwaitExpression(n,call('__await',[ts.visitNode(n.expression,visit)]));
  if(ts.isPrefixUnaryExpression(n)&&n.operator===ts.SyntaxKind.PlusToken)return call('__unary',[ts.visitNode(n.operand,visit)]);
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
  if(ts.isVariableDeclaration(n)&&ts.isObjectBindingPattern(n.name)){assert.equal(n.name.elements.length,1);const element=n.name.elements[0];assert.equal(element.name.getText(),'data');return f.updateVariableDeclaration(n,f.createIdentifier('data'),n.exclamationToken,n.type,call('__get',[ts.visitNode(n.initializer,visit),f.createStringLiteral('data')]));}
  if(ts.isVariableDeclaration(n)&&ts.isArrayBindingPattern(n.name))return f.updateVariableDeclaration(n,n.name,n.exclamationToken,n.type,call('__pair',[ts.visitNode(n.initializer,visit)]));
  if(ts.isRegularExpressionLiteral(n))return call('__regex',[n]);
  if(ts.isTemplateExpression(n))return f.updateTemplateExpression(n,n.head,n.templateSpans.map(s=>f.updateTemplateSpan(s,call('__template',[ts.visitNode(s.expression,visit)]),s.literal)));
  if(ts.isBinaryExpression(n)){const kind=n.operatorToken.kind;if(kind===ts.SyntaxKind.InstanceOfKeyword)return call('__instanceof',[ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);if([ts.SyntaxKind.LessThanToken,ts.SyntaxKind.GreaterThanEqualsToken].includes(kind))return call('__compare',[f.createStringLiteral(kind===ts.SyntaxKind.LessThanToken?'less':'greaterEqual'),ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);if(kind===ts.SyntaxKind.EqualsToken&&ts.isPropertyAccessExpression(n.left))return call('__assign',[ts.visitNode(n.left.expression,visit),f.createStringLiteral(n.left.name.text),ts.visitNode(n.right,visit)]);if(kind===ts.SyntaxKind.EqualsToken&&ts.isElementAccessExpression(n.left))return call('__setIndex',[ts.visitNode(n.left.expression,visit),ts.visitNode(n.left.argumentExpression,visit),ts.visitNode(n.right,visit)]);const ops=new Map([[ts.SyntaxKind.PlusToken,'add'],[ts.SyntaxKind.MinusToken,'subtract'],[ts.SyntaxKind.AsteriskToken,'multiply'],[ts.SyntaxKind.GreaterThanToken,'greater'],[ts.SyntaxKind.PlusEqualsToken,'add']]);if(ops.has(kind)){const op=call('__operator',[f.createStringLiteral(ops.get(kind)),ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);return kind===ts.SyntaxKind.PlusEqualsToken?f.createBinaryExpression(n.left,f.createToken(ts.SyntaxKind.EqualsToken),op):op;}}
  if(ts.isNewExpression(n))return call('__construct',[ts.visitNode(n.expression,visit),...(n.arguments??[]).map(a=>ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&globalNames.has(n.expression.text))return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...n.arguments.map(a=>ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
   const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
   const statements=[variable(recv,ts.visitNode(n.expression.expression,visit))];
   if(n.expression.questionDotToken)statements.push(f.createIfStatement(f.createBinaryExpression(recv,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createReturnStatement(f.createIdentifier('undefined'))));
   const args=n.arguments.map(a=>ts.isArrowFunction(a)?call('__predicate',[f.createIdentifier('upperCoin'),f.createIdentifier('tokenIndex'),ts.visitNode(a,visit)]):ts.visitNode(a,visit));
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
  if(ts.isObjectLiteralExpression(n)){const object=f.createIdentifier('__literal');const statements=[variable(object,call('__newObject',[]))];for(const p of n.properties){if(ts.isSpreadAssignment(p)){statements.push(f.createExpressionStatement(call('__spread',[object,ts.visitNode(p.expression,visit)])));continue;}assert(ts.isShorthandPropertyAssignment(p)||ts.isPropertyAssignment(p));const value=ts.isShorthandPropertyAssignment(p)?ts.visitNode(p.name,visit):ts.visitNode(p.initializer,visit);const computed=ts.isComputedPropertyName(p.name);const key=computed?ts.visitNode(p.name.expression,visit):f.createStringLiteral(ts.isStringLiteral(p.name)||ts.isNumericLiteral(p.name)?p.name.text:p.name.getText());if(computed){const k=f.createIdentifier('__computedKey'+statements.length);statements.push(variable(k,key),f.createExpressionStatement(call('__defineIndex',[object,k,value])));}else statements.push(f.createExpressionStatement(call('__define',[object,key,value])));}return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(object)],true))),undefined,[]);}
  return ts.visitEachChild(n,visit,context);
 };return s=>ts.visitNode(s,visit);
}
function compile(source){const context={exports:{}};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);return context.exports;}
const directory=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid');
const constants=compile(fs.readFileSync(path.join(directory,'hyperliquid.constants.ts'),'utf8'));
const response=compile(fs.readFileSync(path.join(directory,'hyperliquid.order-response.ts'),'utf8'));
const coreSource=fs.readFileSync(path.join(directory,'hyperliquid.exchange.ts'),'utf8'),coreAst=ts.createSourceFile('core.ts',coreSource,ts.ScriptTarget.Latest,true);
const helperSource=['parseHyperliquidExchangePayload','maskHyperliquidAddress'].map(name=>{const n=coreAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(coreAst)===name));assert(n,name);return n.getText(coreAst)+`\nexports.${name}=${name};`;}).join('\n');const helpers=compile(helperSource);
const typesFile=path.join(directory,'hyperliquid.types.ts'),typesAst=ts.createSourceFile(typesFile,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true),endpoints=compile(typesAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(typesAst)==='ENDPOINTS')).getText(typesAst)).ENDPOINTS;
const classes={};for(const[kind,methods]of Object.entries(sourceNames)){const file=path.join(directory,kind==='core'?'hyperliquid.exchange.ts':'hyperliquidspot.exchange.ts'),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();const code=methods.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const t=ts.transform(m.initializer??m.body,[transformer]),printed=printer.printNode(m.initializer?ts.EmitHint.Expression:ts.EmitHint.Unspecified,t.transformed[0],ast);t.dispose();return m.initializer?`${name}=${printed};`:`${m.modifiers?.some(m=>m.kind===ts.SyntaxKind.AsyncKeyword)?'async ':''}${name}(${m.parameters.map(p=>p.name.getText(ast)).join(',')})${printed}`;}).join('\n');const context={...bind,exports:{}};vm.runInNewContext(ts.transpileModule(`class ${cls.name.text}{${code}}\nexports.Class=${cls.name.text};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);classes[kind]=context.exports.Class;}
const cases=[];
async function add(kind,name,label,args=[],config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];g={error:tag({label},'error'),...config};tag(Object.prototype,'defaultPrototype');
 const e=tag(new classes[kind](),'exchange');let approved=Object.hasOwn(config,'approved')?config.approved:true;
 Object.defineProperty(e,'_builderFeeApproved',{get(){return approved;}});e._builderFeeLastCheck=Object.hasOwn(config,'lastCheck')?config.lastCheck:tag({maxFeeRateTenthsBps:91.25,approved:true,maxFeeRate:'0.25%',error:'old'},'lastCheck');
 e.options=tag({applicationId:'0xABCDEF1234567890',key:'0xKEY1234567890',secret:'secret',vault:'0xVAULT1234567890',hyperliquidBuilder:{address:'0xABCDEF1234567890',feeTenthsBps:17.5},...config.options},'options');
 const market=Object.hasOwn(config,'market')?config.market:{id:'BTC',isSpot:false,info:{hyperliquidId:7}};
 e.store=tag({marketsBySymbol:tag({BTC:market,undefined:config.undefinedMarket},'marketsBySymbol')},'store');
 e.activeAssetData=tag(new Map([['BTC',Object.hasOwn(config,'active')?config.active:{leverage:{type:'cross',value:17.5}}]]),'activeAssetData');
 let refreshCount=0,invalidateCount=0,fetchCount=0;const signature=Object.hasOwn(config,'signature')?config.signature:tag({r:'R',s:'S',v:27},'signature');
 const promise=(value,reject)=>tag(reject?Promise.reject(g.error):Promise.resolve(value),'promise');
 e.fetchBuilderFeeApproval=tag(function(){fetchCount++;return tag(Promise.resolve().then(()=>{if(config.rejectFetch)throw g.error;approved=Object.hasOwn(config,'afterApproved')?config.afterApproved:true;}),'promise');},'fetchBuilderFeeApproval');
 e.scheduleOrderRefresh=tag(function(){refreshCount++;if(config.rejectRefresh)throw g.error;},'scheduleOrderRefresh');
 e.invalidateFakePositionsCache=tag(function(){invalidateCount++;},'invalidateFakePositionsCache');
 e.generateNonce=tag(function(){return 1700000000001;},'generateNonce');
 if(name!=='signActionOrDefer')e.signActionOrDefer=tag(function(){return promise(signature,config.rejectSign);},'signActionOrDefer');
 const result=Object.hasOwn(config,'postResult')?config.postResult:tag({data:tag({posted:true},'postedData')},'postResult');
 if(config.rejectData){Object.defineProperty(result,'data',{get(){throw g.error;}});}
 e.xhr=tag({post:tag(function(){return promise(result,config.rejectPost);},'post')},'xhr');
 const wallet=tag({wallet:true},'wallet');const signer=tag(function(){return promise(signature,config.rejectSign);},'signL1Action');const makeWallet=tag(function(){return wallet;},'createWalletFromPrivateKey');
 const parser=tag(function(data){if(config.parserThrows)throw config.parserThrows==='unreadable'?new response.HyperliquidUnreadableOrderResponseError('fixture unreadable'):g.error;if(config.rejectIds){return Object.defineProperty({},'orderIds',{get(){throw new response.HyperliquidUnreadableOrderResponseError('getter unreadable');}});}return response.parseHyperliquidOrderResponse(data);},'parseHyperliquidOrderResponse');
 const virtualClock=tag({getCurrentTimeMs:tag(function(){return 1700000000000;},'getCurrentTimeMs')},'virtualClock');
 for(const[n,v]of Object.entries({Number,Array,Boolean,Error,ENDPOINTS:endpoints,virtualClock,resolveHyperliquidBuilder:constants.resolveHyperliquidBuilder,...helpers,parseHyperliquidOrderResponse:parser,HyperliquidUnreadableOrderResponseError:response.HyperliquidUnreadableOrderResponseError,signL1Action:signer,createWalletFromPrivateKey:makeWallet})){if(typeof v==='function'||typeof v==='object'&&v!==null)tag(v,n);globals[n]=v;}
 args=args.map((v,i)=>typeof v==='object'&&v!==null&&!tags.has(v)?tag(v,'argument'+i):v);
 g.state=()=>({approved:native(approved),refreshCount,invalidateCount,fetchCount});
 const input=native(e),inputArgs=args.map(native),initial={approved:native(approved),refreshCount:0,invalidateCount:0,fetchCount:0};let resultValue;
 try{const value=await Reflect.apply(e[name],e,args);resultValue={value:tree(value)};}catch(error){resultValue={throw:native(error)};}
 cases.push({kind,name,label,input,args:inputArgs,calls,initial,result:resultValue,final:{approved:native(approved),refreshCount,invalidateCount,fetchCount}});
}
(async()=>{
 for(const config of [{},{options:{applicationId:''}},{approved:null},{approved:false},{approved:undefined},{approved:1},{approved:null,afterApproved:false},{approved:null,rejectFetch:true},{throwGet:'_builderFeeApproved'},{throwCall:'resolveHyperliquidBuilder'},{options:{hyperliquidBuilder:{address:null,feeTenthsBps:17.5}}},{throwGet:'feeTenthsBps'}])await add('core','getBuilderInfo','approval',[],config);
 const payload={action:{type:'order',grouping:'na',orders:[{a:7}],builder:{b:'0xABCDEF1234567890',f:17.5}}};
 for(const value of[payload,JSON.stringify(payload),undefined,null,{},[],{action:null},{action:()=>{}},{action:{builder:()=>{},orders:[]}}, {action:{type:17.5,grouping:undefined,orders:{}}}])await add('core','getOrderErrorTelemetry','payload',[value,'Builder Fee rejection']);
 for(const config of[{approved:false},{approved:null},{approved:1},{lastCheck:null},{lastCheck:{maxFeeRateTenthsBps:NaN}},{lastCheck:{maxFeeRateTenthsBps:0}},{lastCheck:{maxFeeRateTenthsBps:'91.25'}},{options:{applicationId:'',vault:'',testnet:true}},{throwCall:'maskHyperliquidAddress'},{throwGet:'options'},{throwCall:'resolveHyperliquidBuilder'}])await add('core','getOrderErrorTelemetry','telemetry branch',[payload,'builder fee'],config);
 for(const message of['other failure',undefined,null,17.5])await add('core','getOrderErrorTelemetry','message guard',[payload,message]);
 for(const config of[{},{rejectPost:true},{rejectData:true},{throwGet:'xhr'},{throwGet:'post'},{throwGlobal:'ENDPOINTS'},{throwCall:'post'},{rejectPost:true,rejectRefresh:true},{postResult:null}])await add('core','postOrderAction','post catch',[{action:'payload'}],config);
 for(const data of[{status:'ok',response:{type:'order',data:{statuses:[{resting:{oid:91}},{filled:{oid:17}}]}}},null,undefined,{}, {status:'err',response:'rejected'}, {status:'ok',response:{type:'order',data:{statuses:[{waitingForTrigger:true}]}}}])await add('core','validateResponseAndExtractOrderIds','response',[data]);
 for(const config of[{parserThrows:'unreadable'},{parserThrows:'plain'},{rejectIds:true},{parserThrows:'unreadable',rejectRefresh:true},{parserThrows:'unreadable',throwInstanceof:true},{throwGlobal:'parseHyperliquidOrderResponse'}])await add('core','validateResponseAndExtractOrderIds','response boundary',[{}],config);
 for(const config of[{},{options:{secret:''}},{options:{secret:null}},{options:{vault:'',testnet:true}},{throwGet:'secret'},{throwGlobal:'signL1Action'},{throwCall:'createWalletFromPrivateKey'},{rejectSign:true}])await add('core','signActionOrDefer','sign defer',[{type:'order'},1700000000001],config);
 for(const time of[undefined,null,1700000005000,1700000005001,1700000004999,0,NaN,Infinity,'1700000005000',1700000005000.5])await add('core','scheduleCancel','deadline',[time]);
 for(const config of[{signature:undefined},{options:{vault:''}},{rejectSign:true},{rejectPost:true},{throwCall:'generateNonce'},{throwCall:'isSafeInteger'},{throwCall:'getCurrentTimeMs'}])await add('core','scheduleCancel','deadline boundary',[1700000005000],config);
 for(const kind of['core','spot'])for(const name of['setLeverage','changeMarginType']){
  const args=name==='setLeverage'?['BTC',4.75,undefined]:[true,'BTC'];
  for(const config of[{},{market:null},{market:{id:'BTC',isSpot:true,info:{hyperliquidId:10007}}},{active:null},{active:{}},{active:{leverage:null}},{signature:undefined},{options:{vault:'',testnet:true}},{rejectSign:true},{rejectPost:true},{throwGet:'hyperliquidId'},{throwCall:'get'},{throwCall:'post'},{throwCall:'invalidateFakePositionsCache'}])await add(kind,name,'action',args,config);
  await add(kind,name,'coercion',name==='setLeverage'?['BTC','3.5',null]:[false,'BTC']);
 }
 // Independent counts:12+25+9+12+8+17+15+15+15+15 =143.
 assert.equal(cases.length,143,'independent ten-root expectation');
 assert(cases.some(c=>c.name==='getBuilderInfo'&&c.result.value?.[0]==='literal'&&c.result.value[3][1][1][1]==='4031800000000000'),'BUILDER_NATIVE fee17.5');
 assert(cases.some(c=>c.name==='scheduleCancel'&&c.label==='deadline'&&c.args[0][1]==='4278bcfe57b88000'&&c.final.refreshCount===0&&c.result.value?.[0]==='undefined'),'DEADLINE_FLOOR');
 console.log(JSON.stringify({names,cases,iterationStop:142,scope:'ten request bodies at first-poll/await boundary; imported helpers and JS primitive/Promise assimilation supplied bindings; eager JS invocation/general scheduling/fullclasses unproved'}));
})().catch(error=>{console.error(error);process.exitCode=1;});
