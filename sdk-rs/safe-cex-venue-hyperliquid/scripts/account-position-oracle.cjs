'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const sourceNames={core:['shouldUseSpotCollateralBalance','shouldRequestPortfolioMarginSpotState','shouldTrackSpotHoldings','_getSymbolConfigurationFromAssetData','getFakePositions','invalidateFakePositionsCache','_hasFullSizedPositionOppositeSide','resolveReduceOnly'],spot:['_getSymbolConfigurationFromAssetData','getFakePositions']};
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
function literalTree(v){if(!['literal','coreDefault','spotDefault'].includes(tags.get(v)))return native(v);return ['literal',native(v),Object.keys(v).map(k=>[units(k),literalTree(v[k])])];}
const bind={
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;if(object==null)throw new TypeError('nullish property access');return Reflect.get(Object(object),name,object);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;return undefined;}),
 __call:(fn,receiver,...args)=>op('call',{function:native(fn),receiver:native(receiver),args:args.map(native)},()=>{if(g.throwCall!==undefined&&tags.get(fn)===g.throwCall)throw g.error;return Reflect.apply(fn,receiver,args);}),
 __newObject:()=>op('object',{},()=>tag({},'literal')),
 __define:(object,name,value)=>op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});}),
 __optionalGet:(object,name)=>object==null?undefined:bind.__get(object,name),
 __greaterEqual:(left,right)=>op('ge',{left:native(left),right:native(right)},()=>left>=right),
 __predicate:(symbol,targetSide,amount,fn)=>op('predicate',{symbol:native(symbol),targetSide:native(targetSide),amount:native(amount)},()=>tag(fn,'oppositePredicate')),
};
const globalNames=new Set(['Boolean','MarginType','PositionMode','OrderSide','PositionSide','HyperliquidExchange','HyperliquidSpotExchange']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isTypeNode(n))return n;
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&ts.isPropertyAccessExpression(n.left))return call('__assign',[ts.visitNode(n.left.expression,visit),f.createStringLiteral(n.left.name.text),ts.visitNode(n.right,visit)]);
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.GreaterThanEqualsToken)return call('__greaterEqual',[ts.visitNode(n.left,visit),ts.visitNode(n.right,visit)]);
 if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&globalNames.has(n.expression.text))return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...n.arguments.map(a=>ts.visitNode(a,visit))]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
  const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
  const vars=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,ts.visitNode(n.expression.expression,visit))],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,call('__get',[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const))];
  const body=f.createBlock([...vars,f.createReturnStatement(call('__call',[fn,recv,...n.arguments.map(a=>ts.isArrowFunction(a)?call('__predicate',[f.createIdentifier('symbol'),f.createIdentifier('targetSide'),f.createIdentifier('amount'),ts.visitNode(a,visit)]):ts.visitNode(a,visit))]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 if(ts.isPropertyAccessExpression(n)){
  if(ts.isPropertyAccessExpression(n.expression)&&n.expression.questionDotToken){const recv=f.createIdentifier('__optionalReceiver');const body=f.createBlock([f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,ts.visitNode(n.expression.expression,visit))],ts.NodeFlags.Const)),f.createReturnStatement(f.createConditionalExpression(f.createBinaryExpression(recv,f.createToken(ts.SyntaxKind.EqualsEqualsToken),f.createNull()),f.createToken(ts.SyntaxKind.QuestionToken),f.createIdentifier('undefined'),f.createToken(ts.SyntaxKind.ColonToken),call('__get',[call('__get',[recv,f.createStringLiteral(n.expression.name.text)]),f.createStringLiteral(n.name.text)])))],true);return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);}
  return call(n.questionDotToken?'__optionalGet':'__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
 }
 if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
 if(ts.isObjectLiteralExpression(n)){
  const object=f.createIdentifier('__literal');const statements=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(object,undefined,undefined,call('__newObject',[]))],ts.NodeFlags.Const))];
  for(const p of n.properties){assert(ts.isPropertyAssignment(p));statements.push(f.createExpressionStatement(call('__define',[object,f.createStringLiteral(p.name.getText()),ts.visitNode(p.initializer,visit)])));}
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),f.createBlock([...statements,f.createReturnStatement(object)],true))),undefined,[]);
 }
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const classes={};
const enumsFile=path.join(root,'packages/safe-cex/src/types.ts'),enumsSource=fs.readFileSync(enumsFile,'utf8'),enumsAst=ts.createSourceFile(enumsFile,enumsSource,ts.ScriptTarget.Latest,true);
const enumNames=['MarginType','PositionMode','OrderSide','PositionSide'];
const enumText=enumNames.map(name=>{const n=enumsAst.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text===name);assert(n,name);return n.getText(enumsAst);}).join('\n');
for(const [kind,names] of Object.entries(sourceNames)){
 const stem=kind==='spot'?'hyperliquidspot':'hyperliquid',file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.exchange.ts`),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
 const configName=kind==='spot'?'SPOT_SYMBOL_CONFIG':'DEFAULT_SYMBOL_CONFIG',config=cls.members.find(m=>m.name?.getText(ast)===configName);assert(config);
 const methods=names.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const node=m.initializer??m.body;const transformed=ts.transform(node,[transformer]);const printed=printer.printNode(m.initializer?ts.EmitHint.Expression:ts.EmitHint.Unspecified,transformed.transformed[0],ast);transformed.dispose();return m.initializer?`${name} = ${printed};`:`${name}(${m.parameters.map(p=>p.name.getText(ast)).join(',')}) ${printed}`;}).join('\n');
 const context={...bind,exports:{}};
 vm.runInNewContext(ts.transpileModule(`${enumText}\nclass ${cls.name.text}{${config.getText(ast)}\n${methods}}\nexports.Exchange=${cls.name.text};exports.config=${cls.name.text}.${configName};${enumNames.map(n=>`exports.${n}=${n};`).join('')}`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
 classes[kind]=context.exports;
}
const cases=[];
function add(kind,name,label,args=[],config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];
 const exchange=tag(new classes[kind].Exchange(),'exchange'),error=tag({label},'error');g={kind,error,...config};
 const selected=exchange[name],generated=tag([],'generatedPositions'),cached=tag([],'cachedPositions');
 const raw={cache:Object.hasOwn(config,'cache')?config.cache:cached};let cacheReads=0,modeReads=0,leverageReads=0;
 Object.defineProperty(exchange,'_fakePositionsCache',{get(){if(config.freshCache&&++cacheReads===2)return generated;return raw.cache;},set(v){if(config.rejectSet)throw tag(new TypeError('cache rejected'),'error:TypeError');raw.cache=v;}});
 Object.defineProperty(exchange,'userAbstractionMode',{get(){return config.freshMode?(++modeReads===1?'standard':'portfolioMargin'):config.mode;}});
 exchange.options=tag({enableSpotMarkets:config.enable},'options');
 const positions=config.positions??[{symbol:'BTC',side:'short',contracts:91.25}];
 exchange.store=tag({markets:tag([{id:'BTC'}],'markets'),positions:tag(positions,'positions')},'store');tag(positions.some,'some');
 let asset=Object.hasOwn(config,'asset')?config.asset:{leverage:{type:'isolated',value:17.5}};
 if(config.freshLeverage)asset={get leverage(){return ++leverageReads===1?{type:'isolated'}:{value:91.25}}};
 const map=tag({has:tag(function(){return config.has??true;},'has'),get:tag(function(){return asset;},'get')},'activeMap');exchange.activeAssetData=map;
 exchange.generateFakePositionsFromMarkets=tag(function(){return generated;},'generateFakePositionsFromMarkets');
 exchange.shouldUseSpotCollateralBalance=tag(function(){return config.collateral;},'shouldUseSpotCollateralBalance');
 globals.Boolean=tag(Boolean,'Boolean');
 for(const n of enumNames)globals[n]=tag(classes[kind][n],n);
 globals.HyperliquidExchange=tag(classes.core.Exchange,'coreClass');globals.HyperliquidSpotExchange=tag(classes.spot.Exchange,'spotClass');
 tag(classes.core.config,'coreDefault');tag(classes.spot.config,'spotDefault');
 const input=native(exchange),inputArgs=args.map(native),initialCache=native(raw.cache);
 const initialPositions=Array.from({length:positions.length},(_,i)=>i in positions?native(positions[i]):['hole']);
 const configs=[classes.core.config,classes.spot.config].map(v=>({handle:native(v),fields:Object.keys(v).map(k=>[k,native(v[k])])}));
 let result,thrown;try{result=Reflect.apply(selected,exchange,args);}catch(e){thrown=e;}
 cases.push({kind,name,label,someCallThrows:config.throwCall==='some',input,args:inputArgs,calls,initialCache,initialPositions,configs,result:thrown?{throw:native(thrown)}:{value:literalTree(result)},finalCache:native(raw.cache)});
}
const modes=[undefined,null,false,0,'','standard','unifiedAccount','portfolioMargin','UnifiedAccount',{},['unifiedAccount']];
for(const mode of modes){add('core','shouldUseSpotCollateralBalance','mode',[],{mode});add('core','shouldRequestPortfolioMarginSpotState','mode',[],{mode});}
add('core','shouldUseSpotCollateralBalance','fresh mode',[],{freshMode:true});add('core','shouldUseSpotCollateralBalance','mode getter throws',[],{throwGet:'userAbstractionMode'});add('core','shouldRequestPortfolioMarginSpotState','mode getter throws',[],{throwGet:'userAbstractionMode'});
for(const collateral of [true,false,0,'',17.5,{},undefined])add('core','shouldTrackSpotHoldings','short circuit',[],{collateral,enable:true});
for(const enable of [undefined,null,false,0,NaN,'',true,91.25,{}])add('core','shouldTrackSpotHoldings','enable truthiness',[],{collateral:false,enable});
for(const config of [{throwCall:'shouldUseSpotCollateralBalance'},{collateral:false,throwGlobal:'Boolean'},{collateral:false,throwGet:'options'},{collateral:false,throwCall:'Boolean'}])add('core','shouldTrackSpotHoldings','guard failure',[],config);
const assets=[undefined,null,{}, {leverage:undefined},{leverage:null},{leverage:{type:'isolated',value:17.5}},{leverage:{type:'cross',value:91.25}},{leverage:{type:'isolated',value:0}},{leverage:{type:'isolated',value:-0}},{leverage:{type:'isolated',value:NaN}},{leverage:{type:'isolated',value:Infinity}},{leverage:{type:'isolated',value:'17.5'}},{leverage:{type:'isolated',value:{}}}];
for(const asset of assets)add('core','_getSymbolConfigurationFromAssetData','asset fields',['BTC'],{asset});
for(const config of [{has:false},{freshLeverage:true},{throwCall:'has'},{throwCall:'get'},{throwGet:'type'},{throwGet:'value'},{has:false,throwGlobal:'HyperliquidExchange'}])add('core','_getSymbolConfigurationFromAssetData','configuration boundary',['BTC'],config);
for(const config of [{},{throwGlobal:'HyperliquidSpotExchange'},{throwGet:'SPOT_SYMBOL_CONFIG'}])add('spot','_getSymbolConfigurationFromAssetData','spot config',['ignored'],config);
for(const cache of [undefined,null,false,0,-0,NaN,'',true,91.25,{},[]])add('core','getFakePositions','cache truthiness',[],{cache});
for(const config of [{freshCache:true},{cache:null,throwCall:'generateFakePositionsFromMarkets'},{cache:null,throwSet:'_fakePositionsCache'},{cache:null,rejectSet:true},{throwGet:'_fakePositionsCache'}])add('core','getFakePositions','cache effects',[],config);
for(const config of [{},{rejectSet:true},{throwSet:'_fakePositionsCache'}])add('core','invalidateFakePositionsCache','invalidation',[],config);
for(const config of [{},{throwGet:'store'},{throwGet:'markets'},{throwCall:'generateFakePositionsFromMarkets'}])add('spot','getFakePositions','uncached generator',[],config);
for(const isSpot of [undefined,null,false,0,NaN,'',true,17.5,{}])add('core','resolveReduceOnly','market flag',[{isSpot},true]);
for(const requested of [undefined,false,0,'requested',{}])add('core','resolveReduceOnly','requested alias',[{isSpot:false},requested]);add('core','resolveReduceOnly','market getter',[{isSpot:false},true],{throwGet:'isSpot'});
const positions=[[],[{symbol:'BTC',side:'short',contracts:91.25}],[{symbol:'BTC',side:'long',contracts:91.25}],[{symbol:'ETH',side:'short',contracts:91.25}],[{symbol:'BTC',side:'short',contracts:0}],[{symbol:'BTC',side:'short',contracts:-0}],[{symbol:'BTC',side:'short',contracts:NaN}],[{symbol:'BTC',side:'short',contracts:'0'}],[{symbol:'BTC',side:'short',contracts:17.5}],[null],[{symbol:'ETH',side:'short',contracts:91.25},{symbol:'BTC',side:'short',contracts:91.25}]];
for(const values of positions)add('core','_hasFullSizedPositionOppositeSide','candidate predicates',['BTC','buy',17.5],{positions:values});
for(const amount of [0,-0,NaN,Infinity,91.25,92.25])add('core','_hasFullSizedPositionOppositeSide','amount comparison',['BTC','buy',amount]);
for(const side of ['sell','SELL',undefined])add('core','_hasFullSizedPositionOppositeSide','target side',['BTC',side,17.5]);
for(const config of [{throwCall:'some'},{throwGet:'contracts'},{throwGet:'symbol'}])add('core','_hasFullSizedPositionOppositeSide','predicate errors',['BTC','buy',17.5],config);
const baseline=cases.find(c=>c.kind==='core'&&c.name==='_hasFullSizedPositionOppositeSide'&&c.label==='candidate predicates'&&c.initialPositions.length===1&&c.result.value?.[1]===true);
assert(baseline,'OPPOSITE_BASELINE true native predicate');
assert(baseline.calls.some(c=>c.op==='ge'),'OPPOSITE_BASELINE owned predicate relational call');
const loaded=cases.find(c=>c.name==='_getSymbolConfigurationFromAssetData'&&c.result.value?.[0]==='literal'&&c.result.value[2].some(([key,v])=>String.fromCharCode(...key)==='leverage'&&v[1]==='4031800000000000'));
assert(loaded,'CONFIG_BASELINE native isolated/17.5 path');
assert.equal(cases.length,129,'independent25+20+20+3+16+3+4+15+23');
console.log(JSON.stringify({names:Object.entries(sourceNames).flatMap(([kind,names])=>names.map(name=>kind+'.'+name)),cases,iterationStop:128,scope:'ten account/position guard/config/cache leaves plus owned some predicate; full generators/classes/intrinsic collections/global primitives unproved'}));
