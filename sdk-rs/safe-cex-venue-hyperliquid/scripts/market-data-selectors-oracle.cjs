'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../..');
const sourceNames={core:['buildMarketsLowerCaseMap','resolveCoinToSymbol','resolveSpotDisplayName','extractClearinghouseBalanceSummary','responseHasDeferredOrderStatus','priceToPrecision'],spot:['symbolOrIdToCoin','priceToPrecision'],private:['getStaleCheckChannelKey','collectHip3DexesFromFills']};
const names=['core.buildMarketsLowerCaseMap','core.resolveCoinToSymbol','spot.symbolOrIdToCoin','core.resolveSpotDisplayName','core.extractClearinghouseBalanceSummary','core.responseHasDeferredOrderStatus','private.getStaleCheckChannelKey','private.collectHip3DexesFromFills','core.priceToPrecision','spot.priceToPrecision'];
let g,ids,tags,nextId,calls;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),tag=(v,t)=>(tags.set(v,t),v);
function native(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['bool',v];if(typeof v==='string')return['string',units(v)];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'nan':b.toString('hex')];}if(!ids.has(v))ids.set(v,nextId++);return['ref',ids.get(v),tags.get(v)||typeof v];}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(error){e.throw=native(error);throw error;}}
function tree(v){if(tags.get(v)==='literal')return['literal',native(v),Object.keys(v).map(k=>[units(k),tree(v[k])])];if(tags.get(v)==='newSet')return['set',native(v),Array.from(v,native)];return native(v);}
const globals={};
const bind={
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish get');const value=Reflect.get(Object(object),name,object);if(typeof value==='function')tag(value,name);return value;}),
 __optionalGet:(object,name)=>object==null?undefined:bind.__get(object,name),
 __optionalIndex:(object,key)=>object==null?undefined:bind.__index(object,key),
 __index:(object,key)=>op('index',{object:native(object),key:native(key)},()=>{if(g.throwIndex)throw g.error;if(object==null)throw new TypeError('nullish index');return Reflect.get(Object(object),key,object);}),
 __call:(fn,receiver,...args)=>op('call',{function:native(fn),receiver:native(receiver),args:args.map(native)},()=>{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;return Reflect.apply(fn,receiver,args);}),
 __construct:(fn,...args)=>op('construct',{function:native(fn),args:args.map(native)},()=>tag(Reflect.construct(fn,args),'newSet')),
 __newObject:()=>op('object',{},()=>tag({},'literal')),
 __define:(object,name,value)=>op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});}),
 __regex:value=>op('regex',{pattern:value.source},()=>tag(value,'unitRegex')),
 __first:value=>op('first',{value:native(value)},()=>{const [first]=value;return first;}),
 __template:value=>op('template',{value:native(value)},()=>`${value}`),
 __subtract:(left,right)=>op('subtract',{left:native(left),right:native(right)},()=>left-right),
 __iterate:value=>op('iterator',{value:native(value)},()=>{
  const real=value[Symbol.iterator]();if(real===null||(typeof real!=='object'&&typeof real!=='function'))throw new TypeError('iterator object');const nextMethod=real.next;let count=0;
  const iterator=tag({[Symbol.iterator](){return this;},next(){const e={op:'next',args:{iterator:native(iterator)}};calls.push(e);try{if(++count===g.throwNext)throw g.error;const step=Reflect.apply(nextMethod,real,[]);if(step===null||(typeof step!=='object'&&typeof step!=='function'))throw new TypeError('iterator result object');const done=!!step.done,value=done?undefined:step.value;e.result=done?['done']:native(value);return{done,value};}catch(error){e.throw=native(error);throw error;}},return(){return op('close',{iterator:native(iterator)},()=>{if(g.throwClose)throw g.closeError;const close=real.return;if(close==null)return{};const result=Reflect.apply(close,real,[]);if(result===null||(typeof result!=='object'&&typeof result!=='function'))throw new TypeError('iterator return object');return result;});}},'iterator');return iterator;
 }),
};
const globalNames=new Set(['LogSeverity','Number','Math','String','Set','numberToString','decimalToPrecision','precisionFromString','ROUND','SIGNIFICANT_DIGITS','DECIMAL_PLACES','NO_PADDING','SPOT_MAX_PRICE_DECIMALS','PERP_MAX_PRICE_DECIMALS','hyperliquidOrderResponseHasDeferredOrderStatus']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args),variable=(name,value)=>f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(name,undefined,undefined,value)],ts.NodeFlags.Const));
 const visit=n=>{
  if(ts.isTypeNode(n))return n;
  if(ts.isForOfStatement(n))return f.updateForOfStatement(n,n.awaitModifier,n.initializer,call('__iterate',[ts.visitNode(n.expression,visit)]),ts.visitNode(n.statement,visit));
  if(ts.isVariableDeclaration(n)&&ts.isArrayBindingPattern(n.name))return f.updateVariableDeclaration(n,n.name,n.exclamationToken,n.type,f.createArrayLiteralExpression([call('__first',[ts.visitNode(n.initializer,visit)])]));
  if(ts.isRegularExpressionLiteral(n))return call('__regex',[n]);
  if(ts.isTemplateExpression(n))return f.updateTemplateExpression(n,n.head,n.templateSpans.map(s=>f.updateTemplateSpan(s,call('__template',[ts.visitNode(s.expression,visit)]),s.literal)));
  if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.MinusToken)return call('__subtract',[ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);
  if(ts.isNewExpression(n))return call('__construct',[ts.visitNode(n.expression,visit),...(n.arguments??[]).map(a=>ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&globalNames.has(n.expression.text))return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...n.arguments.map(a=>ts.visitNode(a,visit))]);
  if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
   const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');const statements=[variable(recv,ts.visitNode(n.expression.expression,visit)),variable(fn,call('__get',[recv,f.createStringLiteral(n.expression.name.text)])),f.createReturnStatement(call('__call',[fn,recv,...n.arguments.map(a=>ts.visitNode(a,visit))]))];
   return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock(statements,true))),undefined,[]);
  }
  if(ts.isPropertyAccessExpression(n)||ts.isElementAccessExpression(n)){
   const property=ts.isPropertyAccessExpression(n),key=property?f.createStringLiteral(n.name.text):ts.visitNode(n.argumentExpression,visit);
   if(ts.isPropertyAccessExpression(n.expression)&&n.expression.questionDotToken){const receiver=f.createIdentifier('__optionalReceiver');const inner=call('__get',[receiver,f.createStringLiteral(n.expression.name.text)]);const tail=call(property?(n.questionDotToken?'__optionalGet':'__get'):(n.questionDotToken?'__optionalIndex':'__index'),[inner,key]);const ret=f.createConditionalExpression(f.createBinaryExpression(receiver,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createToken(ts.SyntaxKind.QuestionToken),f.createIdentifier('undefined'),f.createToken(ts.SyntaxKind.ColonToken),tail);return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([variable(receiver,ts.visitNode(n.expression.expression,visit)),f.createReturnStatement(ret)],true))),undefined,[]);}
   const get=call(property?'__get':'__index',[ts.visitNode(n.expression,visit),key]);
   if(n.questionDotToken){const receiver=f.createIdentifier('__optionalReceiver');const ret=f.createConditionalExpression(f.createBinaryExpression(receiver,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createToken(ts.SyntaxKind.QuestionToken),f.createIdentifier('undefined'),f.createToken(ts.SyntaxKind.ColonToken),call(property?'__get':'__index',[receiver,key]));return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([variable(receiver,ts.visitNode(n.expression,visit)),f.createReturnStatement(ret)],true))),undefined,[]);}return get;
  }
  if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
  if(ts.isObjectLiteralExpression(n)){const object=f.createIdentifier('__literal');const statements=[variable(object,call('__newObject',[]))];for(const p of n.properties){assert(ts.isShorthandPropertyAssignment(p)||ts.isPropertyAssignment(p));const value=ts.isShorthandPropertyAssignment(p)?ts.visitNode(p.name,visit):ts.visitNode(p.initializer,visit);statements.push(f.createExpressionStatement(call('__define',[object,f.createStringLiteral(p.name.getText()),value])));}return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(object)],true))),undefined,[]);}
  return ts.visitEachChild(n,visit,context);
 };return s=>ts.visitNode(s,visit);
}
function compile(source,context={exports:{}}){vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);return context.exports;}
const ccxtFile=path.join(root,'packages/ccxt-compat/src/functions/ccxt_exchange.ts'),ccxtSource=fs.readFileSync(ccxtFile,'utf8'),ccxtAst=ts.createSourceFile(ccxtFile,ccxtSource,ts.ScriptTarget.Latest,true);const ccxt=compile(ccxtAst.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text==='numberToString').getText(ccxtAst));
const numeric=compile(fs.readFileSync(path.join(root,'packages/ccxt-compat/src/functions/number.ts'),'utf8'));
const response=compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-response.ts'),'utf8'));
const severity=compile(fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8')).LogSeverity;
const constantsFile=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.constants.ts'),constantsSource=fs.readFileSync(constantsFile,'utf8'),constantsAst=ts.createSourceFile(constantsFile,constantsSource,ts.ScriptTarget.Latest,true);
const constantValues={};for(const name of ['SPOT_MAX_PRICE_DECIMALS','PERP_MAX_PRICE_DECIMALS']){const n=constantsAst.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.getText(constantsAst)===name);assert(n,name);constantValues[name]=vm.runInNewContext(n.initializer.getText(constantsAst));}
const classes={};for(const [kind,methods] of Object.entries(sourceNames)){const stem=kind==='core'?'hyperliquid.exchange':kind==='spot'?'hyperliquidspot.exchange':'hyperliquid.ws-private',file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.ts`),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();const text=methods.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const transformed=ts.transform(m.initializer??m.body,[transformer]);const printed=printer.printNode(m.initializer?ts.EmitHint.Expression:ts.EmitHint.Unspecified,transformed.transformed[0],ast);transformed.dispose();return m.initializer?`${name}=${printed};`:`${name}(${m.parameters.map(p=>p.name.getText(ast)).join(',')})${printed}`;}).join('\n');const context={...bind,exports:{}};compile(`class ${cls.name.text}{${text}}\nexports.Class=${cls.name.text};`,context);classes[kind]=context.exports.Class;}
const cases=[];
function add(kind,name,label,args=[],config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];g={error:tag({label},'error'),closeError:tag({close:true},'closeError'),...config};
 const exchange=tag(new classes[kind](),'exchange'),old=tag({id:'old',symbol:'OLD-PERP'},'oldMarket'),fallback=tag({id:'DEX:BTC',symbol:'DEX|BTC-PERP'},'fallbackMarket'),direct=tag({id:'BTC',symbol:'BTC-PERP'},'directMarket');
 const map=tag(new Map([['old',old],['dex:btc',fallback]]),'lowerMap');exchange.marketsLowerCaseMap=map;
 const byId={BTC:direct};if(Object.hasOwn(config,'direct'))byId.FOO=config.direct;if(Object.hasOwn(config,'marketDex'))byId.BTC={info:{hip3Dex:config.marketDex}};
 const market=Object.hasOwn(config,'market')?config.market:{isSpot:false,precision:{amount:0.01}};
 exchange.store=tag({marketsById:tag(byId,'marketsById'),marketsBySymbol:tag({BTC:market},'marketsBySymbol')},'store');exchange.parent=tag({store:exchange.store},'parent');exchange.log=tag(function(){},'log');
 const tokenNames=tag(new Set(config.names??[]),'tokenNames');
 if(name==='resolveSpotDisplayName')args[1]=tokenNames;
 if(config.freshFullName){let n=0;Object.defineProperty(args[0],'fullName',{get(){return ++n===1?'Unit First':'Unit Second';}});}
 if(config.freshDex){let n=0;Object.defineProperty(args[1],'dex',{get(){return ++n===1?'FIRST':'SECOND';}});}
 if(config.freshSummary){let n=0;Object.defineProperty(args[0],'marginSummary',{get(){return ++n===1?{accountValue:1,totalMarginUsed:2}:{accountValue:91.25,totalMarginUsed:17.5};}});}
 for(const [n,v] of Object.entries({String,Number,Math,Set,LogSeverity:severity,numberToString:ccxt.numberToString,decimalToPrecision:numeric.decimalToPrecision,precisionFromString:numeric.precisionFromString,ROUND:numeric.ROUND,SIGNIFICANT_DIGITS:numeric.SIGNIFICANT_DIGITS,DECIMAL_PLACES:numeric.DECIMAL_PLACES,NO_PADDING:numeric.NO_PADDING,...constantValues,hyperliquidOrderResponseHasDeferredOrderStatus:response.hyperliquidOrderResponseHasDeferredOrderStatus})){if(typeof v==='function'||typeof v==='object'&&v!==null)tag(v,n);globals[n]=v;}
 args=args.map((v,i)=>typeof v==='object'&&v!==null&&!tags.has(v)?tag(v,'argument'+i):v);
 const input=native(exchange),inputArgs=args.map(native),initialLower=Array.from(map,([k,v])=>[native(k),native(v)]);let result;try{result={value:tree(Reflect.apply(exchange[name],exchange,args))};}catch(error){result={throw:native(error)};}
 cases.push({kind,name,label,input,args:inputArgs,calls,initialLower,result,finalLower:Array.from(map,([k,v])=>[native(k),native(v)])});
}
for(const markets of [[],[{id:'BTC',symbol:'BTC-PERP'}],[{id:'DEX:BTC',symbol:'FIRST'},{id:'dex:btc',symbol:'SECOND'}],[null],[{id:91.25}],undefined])add('core','buildMarketsLowerCaseMap','markets',[markets]);
for(const config of [{throwGet:'marketsLowerCaseMap'},{throwCall:'clear'},{throwGet:'id'},{throwCall:'toLowerCase'},{throwCall:'set'},{throwNext:1},{throwGet:'log'},{throwCall:'log'},{throwGet:'id',throwClose:true}])add('core','buildMarketsLowerCaseMap','map boundary',[[{id:'BTC'}]],config);
for(const coin of ['BTC','dex:BTC','missing','',91.25,null,{}])add('core','resolveCoinToSymbol','coin',[coin]);
for(const direct of [undefined,null,false,0,{symbol:'MATCH'},{symbol:17.5}])add('core','resolveCoinToSymbol','direct value',['FOO'],{direct});
for(const config of [{throwGet:'store'},{throwGet:'marketsById'},{throwGet:'marketsLowerCaseMap'},{throwCall:'get'}])add('core','resolveCoinToSymbol','lookup boundary',['dex:BTC'],config);
for(const symbol of ['BTC-PERP','dex|BTC-PERP','BTC/USDC','-BTC','BTC--PERP','','\ud800-PERP',null,undefined,91.25])add('spot','symbolOrIdToCoin','symbol',[symbol]);
for(const throwCall of ['includes','split'])add('spot','symbolOrIdToCoin','string boundary',['BTC-PERP'],{throwCall});
for(const token of [{name:'UBTC',fullName:'Unit Bitcoin'},{name:'UBTC',fullName:'UNIT Bitcoin'},{name:'U1',fullName:'Unit 1'},{name:'Ubtc',fullName:'Unit x'},{name:'U',fullName:'Unit x'},{name:'UX',fullName:'Unit x'},{name:'UBTC',fullName:null},null,{}, {name:'UBTC',fullName:1001}])add('core','resolveSpotDisplayName','token',[token]);
add('core','resolveSpotDisplayName','collision',[{name:'UBTC',fullName:'Unit Bitcoin'}],{names:['BTC']});add('core','resolveSpotDisplayName','fresh full name',[{name:'UBTC'}],{freshFullName:true});
for(const config of [{throwGet:'fullName'},{throwGet:'name'},{throwCall:'startsWith'},{throwCall:'has'},{throwCall:'test'}])add('core','resolveSpotDisplayName','display boundary',[{name:'UBTC',fullName:'Unit Bitcoin'}],config);
for(const data of [undefined,null,{}, {marginSummary:{accountValue:91.25,totalMarginUsed:17.5}}, {crossMarginSummary:{accountValue:'91.25',totalMarginUsed:'17.5'}}, {marginSummary:{accountValue:null,totalMarginUsed:false}}, {marginSummary:{accountValue:'NaN',totalMarginUsed:1}}, {marginSummary:{accountValue:1,totalMarginUsed:Infinity}}, {marginSummary:false,crossMarginSummary:{accountValue:1,totalMarginUsed:2}}, {marginSummary:{}}])add('core','extractClearinghouseBalanceSummary','summary',[data]);
add('core','extractClearinghouseBalanceSummary','fresh summary',[{}],{freshSummary:true});for(const config of [{throwGet:'marginSummary'},{throwGlobal:'Number'},{throwGet:'accountValue'},{throwCall:'Number'},{throwCall:'isFinite'}])add('core','extractClearinghouseBalanceSummary','summary boundary',[{marginSummary:{accountValue:91.25,totalMarginUsed:17.5}}],config);
for(const data of [undefined,null,{}, {statuses:['waitingForFill']},{response:{data:{statuses:['waitingForTrigger']}}},{statuses:[{resting:{oid:12}}]},{statuses:['unknown']}])add('core','responseHasDeferredOrderStatus','deferred',[data]);add('core','responseHasDeferredOrderStatus','callee error',[{}],{throwCall:'hyperliquidOrderResponseHasDeferredOrderStatus'});
for(const args of [['clearinghouseState',{dex:'DEX'}],['clearinghouseState',{dex:''}],['clearinghouseState',null],['clearinghouseState',{dex:17.5}],['orderUpdates',{dex:'DEX'}],[91.25,{dex:'DEX'}],[null,{dex:'X'}],['clearinghouseState',{dex:Symbol('D')}]])add('private','getStaleCheckChannelKey','channel',args);
add('private','getStaleCheckChannelKey','fresh dex',['clearinghouseState',{}],{freshDex:true});for(const config of [{throwGet:'dex'},{throwGlobal:'String'},{throwCall:'toLowerCase'}])add('private','getStaleCheckChannelKey','routing boundary',['clearinghouseState',{dex:'DEX'}],config);
for(const fills of [[],[{coin:'DEX:BTC'},{coin:'dex:ETH'}],[{coin:'BTC'}],[{coin:':BTC'}],[{coin:'DEX:'}],[null,{coin:0},{coin:'BTC'}],[{coin:'missing'}],undefined])add('private','collectHip3DexesFromFills','fills',[fills]);
for(const marketDex of [undefined,null,0,'','MiXeD',{}])add('private','collectHip3DexesFromFills','market dex',[[{coin:'BTC'}]],{marketDex});for(const config of [{throwGlobal:'Set'},{throwGet:'marketsById'},{throwGet:'coin'},{throwCall:'add'},{throwNext:1},{throwGet:'coin',throwClose:true}])add('private','collectHip3DexesFromFills','fill boundary',[[{coin:'DEX:BTC'}]],config);
for(const kind of ['core','spot']){
 for(const price of [undefined,null,0,-0,91.2536789,'123456.789123',1e-8,NaN,Infinity,'','001.23456',0.000123456789])add(kind,'priceToPrecision','price',['BTC',price]);
 for(const market of [null,{isSpot:false,precision:{amount:0.01}},{isSpot:true,precision:{amount:1e-8}},{isSpot:false,precision:{amount:'0.001'}},{isSpot:false},{precision:null}])add(kind,'priceToPrecision','market precision',['BTC',91.2536789],{market});
 for(const config of [{throwGlobal:'numberToString'},{throwCall:'decimalToPrecision'},{throwGet:'store'},{throwGlobal:'precisionFromString'}])add(kind,'priceToPrecision','precision boundary',['BTC',91.2536789],config);
}
assert.equal(cases.length,161,'independent15+17+12+17+16+8+12+20+22+22');
assert(cases.some(c=>c.name==='extractClearinghouseBalanceSummary'&&c.result.value?.[0]==='literal'&&c.result.value[2][0][1][1]==='4056d00000000000'&&c.result.value[2][1][1][1]==='4031800000000000'),'SUMMARY_BASELINE native91.25/17.5');
assert(cases.some(c=>c.name==='collectHip3DexesFromFills'&&c.result.value?.[0]==='set'&&c.result.value[2].length===1),'DEX_BASELINE nonempty nativeSet');
assert(cases.some(c=>c.name==='responseHasDeferredOrderStatus'&&c.result.value?.[1]===true),'DEFERRED_BASELINE actual helpertrue');
for(const [kind,value] of [['core','0.0001'],['spot','0.000123']])assert(cases.some(c=>c.kind===kind&&c.name==='priceToPrecision'&&JSON.stringify(c.result.value)===JSON.stringify(['string',units(value)])),'PRECISION_BASELINE '+kind+' '+value);
console.log(JSON.stringify({names,cases,iterationStop:160,scope:'ten market/data selector algorithms; imported precision/response libraries, iterator/collection/regex/coercion primitives andfullclasses outsideproof'}));
