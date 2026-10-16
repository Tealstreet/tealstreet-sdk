'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../..');
const sourceNames={agent:['switchToCorrectNetwork','signAgentApproval','submitAgentApproval','createAgentWalletWithBrowserWallet','connectBrowserWallet'],builder:['switchToCorrectNetwork','checkBuilderFeeApproved','signBuilderFeeApproval','submitBuilderFeeApproval','createBuilderInfo','approveBuilderFeeWithBrowserWallet']};
const names=Object.entries(sourceNames).flatMap(([k,ns])=>ns.map(n=>k+'.'+n));
let g,ids,tags,nextId,calls,protos;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),tag=(v,t)=>(tags.set(v,t),v);
function native(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['bool',v];if(typeof v==='string')return['string',units(v)];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'nan':b.toString('hex')];}if(!ids.has(v))ids.set(v,nextId++);return['ref',ids.get(v),tags.get(v)||typeof v,typeof v];}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(error){e.throw=native(error);throw error;}}
function tree(v){if(tags.get(v)==='literal')return['literal',native(v),native(Object.getPrototypeOf(v)),Object.keys(v).map(k=>[units(k),tree(v[k])])];return native(v);}
const globals={};
const bind={
 __typeofGlobal:name=>op('typeofGlobal',{name},()=>{if(g.throwTypeof===name)throw g.error;return typeof globals[name];}),
 __await:async value=>{const e={op:'await',args:{value:native(value)}};calls.push(e);try{const result=await value;e.result=native(result);return result;}catch(error){e.throw=native(error);throw error;}},
 __unary:value=>op('unary',{value:native(value)},()=>+value),
 __compare:(operator,left,right)=>op('compare',{operator,left:native(left),right:native(right)},()=>operator==='less'?left<right:left>=right),
 __construct:(constructor,...args)=>op('construct',{constructor:native(constructor),args:args.map(native)},()=>Reflect.construct(constructor,args)),
 __instanceof:(value,constructor)=>op('instanceof',{value:native(value),constructor:native(constructor)},()=>{if(g.throwInstanceof)throw g.error;return value instanceof constructor;}),
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish get');const v=Reflect.get(Object(object),name,object);if(typeof v==='function')tag(v,name);return v;}),
 __optionalGet:(object,name)=>object==null?undefined:bind.__get(object,name),
 __optionalIndex:(object,key)=>object==null?undefined:bind.__index(object,key),
 __index:(object,key)=>op('index',{object:native(object),key:native(key)},()=>{if(object==null)throw new TypeError('nullish index');return Reflect.get(Object(object),key,object);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;}),
 __setIndex:(object,key,value)=>op('setIndex',{object:native(object),key:native(key),value:native(value)},()=>{object[key]=value;}),
 __defineIndex:(object,key,value)=>op('defineIndex',{object:native(object),key:native(key),value:native(value)},()=>{Object.defineProperty(object,key,{value,writable:true,enumerable:true,configurable:true});}),
 __call:(fn,receiver,...args)=>{const input={function:native(fn),receiver:native(receiver),args:args.map(native)};if(['request','signTypedData','stringify','fetch','reportEvent'].includes(tags.get(fn)))input.payloads=args.map(tree);return op('call',input,()=>{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;calls.at(-1).invoked=true;return Reflect.apply(fn,receiver,args);});},
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
const globalNames=new Set(['window','ethers','Date','JSON','Error','fetch','console','parseFloat','isNaN','getBuilderAddress','builderFeeRateFromTenthsBps','hasEthereumWallet','generateAgentWalletName','switchToCorrectNetwork','signAgentApproval','submitAgentApproval','signBuilderFeeApproval','submitBuilderFeeApproval','reportEvent','BASE_URLS','DEFAULT_BUILDER_FEE_RATE','DEFAULT_BUILDER_FEE_TENTHS_BPS','TEALSTREET_BUILDER_FEE','HYPERLIQUID_BUILDER_DOMAIN','HYPERLIQUID_BUILDER_DOMAIN_TESTNET','ARBITRUM_ONE_CHAIN','ARBITRUM_SEPOLIA_CHAIN','APPROVE_AGENT_TYPES','APPROVE_BUILDER_FEE_TYPES']);
let moduleKind;
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args),variable=(name,value)=>f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(name,undefined,undefined,value)],ts.NodeFlags.Const));
 const visit=n=>{
  if(ts.isTypeNode(n))return n;
  if(ts.isAwaitExpression(n)){const expr=ts.visitNode(n.expression,visit);const local=ts.isCallExpression(n.expression)&&ts.isIdentifier(n.expression.expression)&&['trySwitch','tryAddChain'].includes(n.expression.expression.text);return f.updateAwaitExpression(n,local?expr:call('__await',[expr]));}
  if(ts.isTypeOfExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='window')return call('__typeofGlobal',[f.createStringLiteral('window')]);
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
  if(ts.isVariableDeclarationList(n)){
   const declarations=[];
   for(const d of n.declarations){
    if(ts.isObjectBindingPattern(d.name)){const raw=f.createUniqueName('__binding');declarations.push(f.createVariableDeclaration(raw,undefined,undefined,ts.visitNode(d.initializer,visit)));for(const e of d.name.elements){assert(ts.isIdentifier(e.name)&&!e.dotDotDotToken&&!e.initializer);declarations.push(f.createVariableDeclaration(e.name,undefined,undefined,call('__get',[raw,f.createStringLiteral(e.propertyName?.getText()??e.name.text)])));}}
    else declarations.push(ts.visitNode(d,visit));
   }
   return f.updateVariableDeclarationList(n,declarations);
  }
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
  if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text.startsWith('ARBITRUM_')?moduleKind+'.'+n.text:n.text)]);
  if(ts.isArrayLiteralExpression(n)){const array=f.createIdentifier('__array');const statements=[variable(array,call('__newArray',[]))];n.elements.forEach((v,i)=>statements.push(f.createExpressionStatement(call('__arrayDefine',[array,f.createNumericLiteral(i),ts.visitNode(v,visit)]))));return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(array)],true))),undefined,[]);}
  if(ts.isObjectLiteralExpression(n)){const object=f.createIdentifier('__literal');const statements=[variable(object,call('__newObject',[]))];for(const p of n.properties){if(ts.isSpreadAssignment(p)){statements.push(f.createExpressionStatement(call('__spread',[object,ts.visitNode(p.expression,visit)])));continue;}assert(ts.isShorthandPropertyAssignment(p)||ts.isPropertyAssignment(p));const value=ts.isShorthandPropertyAssignment(p)?ts.visitNode(p.name,visit):ts.visitNode(p.initializer,visit);const computed=ts.isComputedPropertyName(p.name);const key=computed?ts.visitNode(p.name.expression,visit):f.createStringLiteral(ts.isStringLiteral(p.name)||ts.isNumericLiteral(p.name)?p.name.text:p.name.getText());if(computed){const k=f.createIdentifier('__computedKey'+statements.length);statements.push(variable(k,key),f.createExpressionStatement(call('__defineIndex',[object,k,value])));}else statements.push(f.createExpressionStatement(call('__define',[object,key,value])));}return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(object)],true))),undefined,[]);}
  return ts.visitEachChild(n,visit,context);
 };return s=>ts.visitNode(s,visit);
}
function compile(text,context={exports:{}}){vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);return context.exports;}
const directory=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid');
const constants=compile(fs.readFileSync(path.join(directory,'hyperliquid.constants.ts'),'utf8'));
const typesSource=fs.readFileSync(path.join(directory,'hyperliquid.types.ts'),'utf8'),typesAst=ts.createSourceFile('types.ts',typesSource,ts.ScriptTarget.Latest,true),baseStatement=typesAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(typesAst)==='BASE_URLS'));const BASE_URLS=compile(baseStatement.getText(typesAst)).BASE_URLS;
const roots={},moduleValues={};
for(const[kind,ns]of Object.entries(sourceNames)){
 const file=path.join(directory,kind==='agent'?'hyperliquid.agent.ts':'hyperliquid.builderFee.ts'),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),printer=ts.createPrinter();
 const values={...constants,...(kind==='agent'?moduleValues.builder:{})};
 for(const n of ast.statements.filter(ts.isVariableStatement))for(const d of n.declarationList.declarations){if(['ARBITRUM_ONE_CHAIN','ARBITRUM_SEPOLIA_CHAIN','APPROVE_AGENT_TYPES','APPROVE_BUILDER_FEE_TYPES','HYPERLIQUID_BUILDER_DOMAIN','HYPERLIQUID_BUILDER_DOMAIN_TESTNET','DEFAULT_BUILDER_FEE_RATE','DEFAULT_BUILDER_FEE_TENTHS_BPS'].includes(d.name.getText(ast))){values[d.name.getText(ast)]=vm.runInNewContext(ts.transpileModule('const value='+d.initializer.getText(ast)+'; value;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{...constants,...values});}}
 moduleValues[kind]=values;roots[kind]={};moduleKind=kind;
 for(const name of ns){const declaration=ast.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name.text===name);assert(declaration,name);const body=ts.transform(declaration.body,[transformer]);const parameters=declaration.parameters.map(p=>{if(!p.initializer)return p.name.getText(ast);const t=ts.transform(p.initializer,[transformer]);const printed=printer.printNode(ts.EmitHint.Expression,t.transformed[0],ast);t.dispose();return p.name.getText(ast)+'='+printed;}).join(',');const printed=printer.printNode(ts.EmitHint.Unspecified,body.transformed[0],ast);body.dispose();const context={...bind,exports:{}};compile(`${declaration.modifiers?.some(m=>m.kind===ts.SyntaxKind.AsyncKeyword)?'async ':''}function ${name}(${parameters})${printed}\nexports.root=${name};`,context);roots[kind][name]=context.exports.root;}
}
// Builder values are needed by the agent imports; domains are the same handles.
moduleValues.agent.HYPERLIQUID_BUILDER_DOMAIN=moduleValues.builder.HYPERLIQUID_BUILDER_DOMAIN;moduleValues.agent.HYPERLIQUID_BUILDER_DOMAIN_TESTNET=moduleValues.builder.HYPERLIQUID_BUILDER_DOMAIN_TESTNET;
const cases=[];
async function add(kind,name,label,args=[],config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];g={...config};g.error=Object.hasOwn(config,'thrown')?config.thrown:tag(new Error(config.rejected?'user rejected fixture':'fixture failure'),'error');tag(Object.prototype,'defaultPrototype');
 const promise=(value,fail)=>tag(fail?Promise.reject(g.error):Promise.resolve(value),'promise');
 const parsed=tag({r:'R-left',s:'S-right',v:28},'parsedSignature');
 const network=tag({success:config.networkSuccess??true,error:config.networkError},'networkResult');
 const signature=tag({r:'R-submitted',s:'S-submitted',v:27},'signature');const signed=tag({signature,nonce:91.25},'signedResult');
 const submitted=tag({success:config.submitSuccess??true,message:'venue result'},'submittedResult');
 const signer=tag({getAddress:tag(function(){return promise(config.address??'0xABCDEF1234567890',config.rejectAddress);},'getAddress'),signTypedData:tag(function(){return promise('raw-signature',config.rejectSign);},'signTypedData')},'signer');
 const provider=tag({send:tag(function(){return promise(null,config.rejectAccounts);},'send'),getSigner:tag(function(){return promise(signer,config.rejectSigner);},'getSigner')},'provider');
 let requests=0;
 const ethereum=tag({request:tag(function(){const fail=(config.requestFailures??[])[requests++];return promise(config.requestValue??null,fail);},'request')},'ethereum');
 const window=Object.hasOwn(config,'window')?config.window:tag({ethereum:config.noEthereum?undefined:ethereum},'window');
 const ethers=tag({Signature:tag({from:tag(function(){return parsed;},'from')},'Signature'),Wallet:tag({createRandom:tag(function(){return tag({address:'0xAGENT1234567890',privateKey:'private-fixture'},'agentWallet');},'createRandom')},'Wallet'),BrowserProvider:tag(function(){if(config.rejectConstruct)throw g.error;return provider;},'BrowserProvider')},'ethers');
 const response=tag({ok:config.ok??true,status:config.status??502,text:tag(function(){return promise(config.text??'91.25',config.rejectText);},'text'),json:tag(function(){return promise(Object.hasOwn(config,'response')?config.response:{status:'ok'},config.rejectJson);},'json')},'response');
 const bindings={...moduleValues.builder,...moduleValues[kind],BASE_URLS,window,ethers,Error,Date:tag({now:tag(()=>1700000000001,'now')},'Date'),JSON,parseFloat,isNaN,console:tag({error:tag(function(){if(config.rejectConsole)throw g.error;},'error')},'console'),fetch:tag(function(){return promise(response,config.rejectFetch);},'fetch'),getBuilderAddress:constants.getBuilderAddress,builderFeeRateFromTenthsBps:constants.builderFeeRateFromTenthsBps,hasEthereumWallet:tag(function(){if(config.rejectHasWallet)throw g.error;return window!==undefined&&!!window?.ethereum;},'hasEthereumWallet'),generateAgentWalletName:tag(function(){return 'Tealstreet abcd';},'generateAgentWalletName'),switchToCorrectNetwork:tag(function(){return promise(network,config.rejectNetwork);},'switchToCorrectNetwork'),signAgentApproval:tag(function(){return promise(signed,config.rejectSign);},'signAgentApproval'),signBuilderFeeApproval:tag(function(){return promise(signed,config.rejectSign);},'signBuilderFeeApproval'),submitAgentApproval:tag(function(){return promise(submitted,config.rejectSubmit);},'submitAgentApproval'),submitBuilderFeeApproval:tag(function(){return promise(submitted,config.rejectSubmit);},'submitBuilderFeeApproval'),reportEvent:tag(function(){if(config.rejectReport)throw g.error;},'reportEvent')};
 for(const n of['ARBITRUM_ONE_CHAIN','ARBITRUM_SEPOLIA_CHAIN']){delete bindings[n];bindings[kind+'.'+n]=moduleValues[kind][n];}
 for(const[n,v]of Object.entries(bindings)){if(typeof v==='function'||typeof v==='object'&&v!==null)tag(v,n);globals[n]=v;}
 args=args.map((v,i)=>typeof v==='object'&&v!==null&&!tags.has(v)?tag(v,'argument'+i):v);
 if(['signAgentApproval','signBuilderFeeApproval'].includes(name))args[0]=signer;
 const inputArgs=args.map(native);let result;try{const value=await roots[kind][name](...args);result={value:tree(value)};}catch(error){result={throw:native(error)};}
 cases.push({kind,name,label,args:inputArgs,calls,result,requests});
}
(async()=>{
 for(const kind of['agent','builder']){
  for(const config of[{},{requestFailures:[true,false,false]},{requestFailures:[true,true]},{requestFailures:[true,false,true]},{requestValue:false},{window:undefined},{noEthereum:true},{throwGet:'request'},{throwTypeof:'window'}])await add(kind,'switchToCorrectNetwork','retry',[false],config);
  await add(kind,'switchToCorrectNetwork','testnet',[true]);
 }
 for(const kind of['agent','builder']){
  const name=kind==='agent'?'signAgentApproval':'signBuilderFeeApproval';
  for(const nonce of[undefined,null,0,-0,91.25])await add(kind,name,'nonce',[undefined,'0xBUILDER','Agent-or-rate',false,nonce]);
  for(const config of[{rejectSign:true},{throwGet:'r'},{throwCall:'from'},{throwGlobal:'Date'}])await add(kind,name,'sign boundary',[undefined,'0xBUILDER',undefined,undefined,undefined],config);
  await add(kind,name,'testnet',[undefined,'0xBUILDER','Agent-or-rate',true,17.5]);
 }
 for(const kind of['agent','builder']){
  const name=kind==='agent'?'submitAgentApproval':'submitBuilderFeeApproval';
  for(const config of[{},{response:{status:'err',response:'denied'}},{response:{status:'err',response:''}},{response:null},{ok:false},{rejectFetch:true},{rejectJson:true},{throwCall:'stringify'},{throwGlobal:'BASE_URLS'},{rejectFetch:true,thrown:{plain:true}}])await add(kind,name,config.response?.status==='err'?'submission denial':'submission',['0xADDRESS','Agent-or-rate',91.25,{r:'R',s:'S',v:27},false],config);
  await add(kind,name,'testnet',['0xADDRESS','Agent-or-rate',17.5,{r:'R',s:'S',v:28},true]);
 }
 for(const config of[{},{text:'17.5'},{text:'NaN'},{text:'Infinity'},{text:'-1.25'},{text:'0'},{ok:false},{rejectFetch:true},{rejectText:true},{rejectFetch:true,rejectConsole:true},{rejectFetch:true,thrown:'plain'},{throwGlobal:'BASE_URLS'},{throwCall:'getBuilderAddress'}])await add('builder','checkBuilderFeeApproved','fee',['0xUSER',undefined,false,17.5],config);
 for(const testnet of[undefined,null,true])await add('builder','checkBuilderFeeApproved','default',['0xUSER','0xADDRESS',testnet,undefined]);
 for(const fee of[undefined,null,0,NaN,Infinity,17.5])await add('builder','createBuilderInfo','fee',[false,'0xABCDEF',fee]);
 for(const address of[undefined,null,'',91.25])await add('builder','createBuilderInfo','address',[true,address,91.25]);
 for(const config of[{},{window:undefined},{noEthereum:true},{rejectConstruct:true},{rejectAccounts:true},{rejectSigner:true},{rejectAddress:true},{rejectAddress:true,rejected:true},{rejectAddress:true,thrown:{plain:true}},{rejectHasWallet:true}])await add('agent','connectBrowserWallet','connect',[],config);
 for(const kind of['agent','builder']){
  const name=kind==='agent'?'createAgentWalletWithBrowserWallet':'approveBuilderFeeWithBrowserWallet';const args=kind==='agent'?['0xABCDEF1234567890',false,undefined]:['0xABCDEF1234567890',false,undefined,undefined];
  for(const config of[{},{window:undefined},{noEthereum:true},{networkSuccess:false},{networkSuccess:false,networkError:'network denied'},{address:'0xOTHER1234567890'},{submitSuccess:false},{rejectNetwork:true},{rejectConstruct:true},{rejectAccounts:true},{rejectSigner:true},{rejectAddress:true},{rejectSign:true},{rejectSubmit:true},{rejectSubmit:true,rejected:true},{rejectSubmit:true,thrown:{plain:true}},{rejectReport:true},{throwCall:kind==='agent'?'generateAgentWalletName':'getBuilderAddress'}])await add(kind,name,config.networkSuccess===false?'network denial':config.address?'wallet mismatch':'browser flow',args,config);
 }
 // Fixed expectation: switch10each/sign10each/submit11each/check16/factory10/connect10/browser18each =134.
 assert.equal(cases.length,134,'independent11root corpus');
 assert(cases.some(c=>c.name==='createBuilderInfo'&&c.result.value?.[3][1][1][1]==='4031800000000000'),'NATIVE_FACTORY fee17.5');
 console.log(JSON.stringify({names,cases,iterationStop:133,scope:'11sourcebodies/localnetworkhelpers, firstpoll/hostawait only; imported constants/helpers actualsource; provider/fetch/ethers/JSON primitive contracts, no livewallet/network/signing/eagerasync proof'}));
})().catch(error=>{console.error(error);process.exitCode=1;});
