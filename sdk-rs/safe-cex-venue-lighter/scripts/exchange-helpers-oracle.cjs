const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
let source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts'),'utf8');
const mutation={
 'minimum-inclusive':['amount < minBaseAmount','amount <= minBaseAmount'],
 'position-zero':['position.side === closingPositionSide && position.contracts > 0','position.side === closingPositionSide && position.contracts >= 0'],
 'closing-side':['return orderSide === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;','return orderSide === OrderSide.Buy ? PositionSide.Long : PositionSide.Short;'],
 'reduce-message':["orderSide === OrderSide.Buy ? 'short' : 'long'","orderSide === OrderSide.Buy ? 'long' : 'short'"],
 'rate-status-float':['Number(record?.statusCode ?? record?.response?.status)','parseFloat(record?.statusCode ?? record?.response?.status)'],
 'slippage-or':['normalizeLighterStopSlippagePct(opts.maxSlippagePct) ??','(normalizeLighterStopSlippagePct(opts.maxSlippagePct) || undefined) ??'],
 'margin-precedence':['info?.market_config?.market_margin_mode ??','info?.market_margin_mode ??'],
 'margin-message':["configuredMarginMode === 1 ? 'isolated' : configuredMarginMode === 0 ? 'cross' : 'the intended margin mode'","configuredMarginMode === 1 ? 'cross' : configuredMarginMode === 0 ? 'isolated' : 'the intended margin mode'"],
 'ohlcv-key':['`${opts.symbol}:${opts.interval}`','`${opts.symbol}|${opts.interval}`'],
 'reference-side':['const sidePrice = side === OrderSide.Buy ? ticker.ask : ticker.bid;','const sidePrice = side === OrderSide.Buy ? ticker.bid : ticker.ask;'],
 'stop-loss-tie':['return order.price <= referencePrice ? OrderType.StopLoss','return order.price < referencePrice ? OrderType.StopLoss'],
 'account-direct':['return BigInt(String(value));','return BigInt(value);'],
 'address-case':["trimmed.match(/0x[a-fA-F0-9]{40}/)","trimmed.match(/0x[a-fA-F0-9]{40}/i)"],
}[process.env.LIGHTER_EXCHANGE_ORACLE_MUTATION];
if(mutation){assert.equal(source.split(mutation[0]).length,2,`exchange mutation anchor ${process.env.LIGHTER_EXCHANGE_ORACLE_MUTATION}`);source=source.replace(...mutation);}
const ast=ts.createSourceFile('exchange.ts',source,ts.ScriptTarget.Latest,true);
const names=['LIGHTER_DEFAULT_QUOTE','LIGHTER_KNOWN_QUOTES','parseLighterMarketSymbol','LIGHTER_MARGIN_FRACTION_TICK','LIGHTER_MAX_STOP_SLIPPAGE_PCT','isLighterPerpMarket','getLighterMarketLeverageMax','parseLighterMarginFractionRatio','normalizeLighterStopSlippagePct'];
const declarations=ast.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(declarations.length,names.length,'pure symbol dependencies');
const exportsObject={};
vm.runInNewContext(ts.transpileModule(declarations.map(n=>n.getText(ast)).join('\n')+'\nexports.parse=parseLighterMarketSymbol;exports.knownQuotes=LIGHTER_KNOWN_QUOTES;exports.perp=isLighterPerpMarket;exports.leverage=getLighterMarketLeverageMax;exports.ratio=parseLighterMarginFractionRatio;exports.slippage=normalizeLighterStopSlippagePct;', {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:exportsObject});
const cases=[];
for(const symbol of ['', '/', '//', '///', '/USDC', 'BTC/', 'BTC//USDG', 'BTC/USDT/PERP', 'BTC/OTHER','BTC','BTCUSDT','BTCUSDC','BTCUSDG','USDT','USDC','USDG','BTCusdc','BTCUSDC ','\uFEFFBTCUSDC','BTCUSDG\uFEFF','😀USDC','USDGUSDT',...exportsObject.knownQuotes.flatMap(quote=>[quote,`BTC${quote}`,`😀${quote}`,`BTC${quote} `])])for(const defaultQuote of [undefined,'USDC','USDG'])cases.push({symbol,defaultQuote,expected:exportsObject.parse(symbol,defaultQuote)});
const outcome=f=>{try{return {ok:f()??null};}catch(error){return {error:{name:error.name,message:error.message}};}};
const inputs=[undefined,null,false,true,0,-0,-1,0.5,1,1.0000000000000002,50,100,100.00000000000001,10000,1e308,'',' ','\uFEFF1.2tail','\u00851.2','0x10','12abc','-0','1e999','5e-324',[],[12],{}, {toString:null},[{toString:0}],'spot','SPOT','SpOt','spot ','perp',1000];
const fractions=inputs.map(value=>({...value===undefined?{}:{value},ratio:outcome(()=>exportsObject.ratio(value)),slippage:outcome(()=>exportsObject.slippage(value))}));
const markets=[];
for(const value of inputs)for(const status of [undefined,'active','inactive','Active'])for(const fields of [{market_type:value},{marketType:value},{market_type:value,marketType:'spot'},{market_type:null,marketType:value}]){const market={status,...fields};markets.push({market,expected:outcome(()=>exportsObject.perp(market))});}
const leverages=[];
const keys=['min_initial_margin_fraction','minInitialMarginFraction','default_initial_margin_fraction','defaultInitialMarginFraction'];
for(const value of inputs)for(let i=0;i<keys.length;i++)for(const fill of [undefined,null,'200']){const market=Object.fromEntries(keys.map((k,j)=>[k,j===i?value:fill]));leverages.push({market,expected:outcome(()=>exportsObject.leverage(market))});}
const helperNames=['resolveMarketOrderReferencePrice','inferStopLossTypeFromReduceOnlyLimit','getOhlcvFallbackKey','getMarketConfiguredMarginMode','buildInvalidMarginModeOrderMessage','resolveStopSlippagePct','isRateLimitOrderError','getReduceOnlyClosingPositionSide','hasOpenReduceOnlyTargetPosition','buildNoPositionReduceOnlyMessage','getOrderMinimumViolationReasons','parseAccountIndex','extractL1AddressCandidate','parseMarginMode','marginModeToMarginType','marginTypeToMarginMode','snapToLeverageTier','getInitialTickDelay','getPositivePriceCandidate','shouldUseBatchedCreateOrders','isWasmRuntimeIssueMessage','isInvalidMarginModeOrderError','isPartnerAttributionTierOrderError','isPartnerAttributionApprovalMissingOrderError'];
const exchangeClass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='LighterExchange');
const helperDeclarations=helperNames.map(name=>{
 const member=exchangeClass.members.find(n=>n.name?.getText(ast)===name);
 assert.ok(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),`LighterExchange.${name} source`);
 return `exports.${name} = ${member.initializer.getText(ast)};`;
});
const leverageTable=exchangeClass.members.find(n=>n.name?.getText(ast)==='VALID_LEVERAGE_TIERS');
assert.ok(leverageTable&&ts.isPropertyDeclaration(leverageTable),'live leverage table');
const tickDelay=ast.statements.filter(ts.isVariableStatement).find(n=>n.declarationList.declarations.some(d=>d.name.getText(ast)==='LIGHTER_INITIAL_TICK_MAX_DELAY_MS'));
assert.ok(tickDelay,'live initial tick cap');
const helperDependencies=`class LighterExchange { ${leverageTable.getText(ast)} }\n${tickDelay.getText(ast)}\nexports.leverageTiers=LighterExchange.VALID_LEVERAGE_TIERS;`;
const slippageDefault=ast.statements.filter(ts.isVariableStatement).find(n=>n.declarationList.declarations.some(d=>d.name.getText(ast)==='LIGHTER_DEFAULT_STOP_SLIPPAGE_PCT'));
assert.ok(slippageDefault,'stop slippage default source');
const stopOptions={extra:{}};
const marketStore={marketsBySymbol:{},tickersBySymbol:{}};

const tickRuntime={interval:0,calls:0};
const tierSource=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.account-tier.ts'),'utf8');
const domain=ts.createSourceFile('types.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const enums=domain.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(domain)).join('\n');
const utilsExports={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.utils.ts'),'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:utilsExports});
vm.runInNewContext(ts.transpileModule(enums+'\n'+tierSource+'\n'+helperDependencies+'\n'+slippageDefault.getText(ast)+'\n'+helperDeclarations.join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:exportsObject,getPositivePriceCandidate:(...values)=>exportsObject.getPositivePriceCandidate(...values),options:stopOptions,store:marketStore,parseMarginMode:value=>exportsObject.parseMarginMode(value),getMarketConfiguredMarginMode:market=>exportsObject.getMarketConfiguredMarginMode(market),normalizeLighterStopSlippagePct:exportsObject.slippage,getReduceOnlyClosingPositionSide:(side)=>exportsObject.getReduceOnlyClosingPositionSide(side),isLighterL1Address:utilsExports.isLighterL1Address,getTickInterval(){tickRuntime.calls++;return tickRuntime.interval;}});
const messages=['','WASM NOT INITIALIZED','failed to fetch dynamically imported module','SignerClient export not found','leverage signer unavailable in runtime','node:fs','prefix NODE:FS suffix','node:FSX','wasm not initialized!','wasm not initialiseD','invalid margin mode','INVALID MARGIN MODE','standard integrator plus','STANDARD PARTNER ATTRIBUTION ACCOUNT TYPE','integrator not approved','integrator is not approved','integrator approved','\uFEFFinvalid margin mode\u0085','ſignerclient export not found'];
const wasmIssues=messages.map(message=>({message,expected:exportsObject.isWasmRuntimeIssueMessage(message)}));
const orderErrors=[];
for(const code of [undefined,null,'',false,0,21613,'21613',' 21613 ','21520','21149','\uFEFF21520\uFEFF','\u008521520',[],['21613'],{}, {toString:null},[{toString:0}]])for(const error of [undefined,null,code,{code},{code,response:{data:{code:'21149'}}},{code:null,response:{data:{code}}},{response:{data:{error:{code}}}},{code:'',response:{data:{code}}},{response:{data:{code:null,error:{code}}}}])for(const message of messages){
 orderErrors.push({error,message,margin:outcome(()=>exportsObject.isInvalidMarginModeOrderError(error,message)),tier:outcome(()=>exportsObject.isPartnerAttributionTierOrderError(error,message)),approval:outcome(()=>exportsObject.isPartnerAttributionApprovalMissingOrderError(error,message))});
}
// Pin conflicts at the second/third fallback boundary, including falsy masks.
for(const code of [undefined,null,'',false,0,'21520','21149',{toString:null}])for(const inner of ['21520','21149',{toString:null}])for(const message of ['', 'standard integrator plus', 'integrator not approved']) {
 const error={response:{data:{code,error:{code:inner}}}};
 orderErrors.push({error,message,margin:outcome(()=>exportsObject.isInvalidMarginModeOrderError(error,message)),tier:outcome(()=>exportsObject.isPartnerAttributionTierOrderError(error,message)),approval:outcome(()=>exportsObject.isPartnerAttributionApprovalMissingOrderError(error,message))});
}
const priceInputs=[undefined,-Infinity,-1,-0,0,Number.MIN_VALUE,1,2,1e308,Infinity,NaN];
const priceCandidates=[{values:[],expected:null}];
for(const a of priceInputs)for(const b of priceInputs)for(const c of [undefined,3,NaN]){const values=[a,b,c];priceCandidates.push({values:values.map(v=>v===undefined?{$undefined:true}:v),expected:exportsObject.getPositivePriceCandidate(...values)??null});}
const baseOrder={symbol:'BTC',type:exportsObject.OrderType.Limit,side:'buy',amount:1,price:1};
const batches=[];
const addBatch=orders=>batches.push({orders,expected:exportsObject.shouldUseBatchedCreateOrders(orders)});
addBatch([]);addBatch([baseOrder]);addBatch([baseOrder,baseOrder]);
const patches=[...Object.values(exportsObject.OrderType).map(type=>({type})),...priceInputs.map(price=>({price})),...[undefined,false,true].map(reduceOnly=>({reduceOnly})),...[undefined,-0,0,1,NaN,Infinity].flatMap(value=>[{stopLoss:value},{takeProfit:value}]),...['BTC','btc','','BTC\uFEFF','BTC\u0085','BTC:ETH','😀'].map(symbol=>({symbol})),{amount:NaN},{side:'sell'},{clientOrderId:'same'},{postOnly:true}];
for(const patch of patches){const order={...baseOrder,...patch};addBatch([order,baseOrder]);addBatch([baseOrder,order]);addBatch([baseOrder,baseOrder,order]);addBatch([order,order]);}
const leverageInputs=[NaN,Infinity,-Infinity,-0,0,-1,Number.MIN_VALUE,Number.MAX_VALUE, -Number.MAX_VALUE];
for(const tier of exportsObject.leverageTiers)leverageInputs.push(tier,tier-Number.EPSILON*tier,tier+Number.EPSILON*tier);
for(let i=1;i<exportsObject.leverageTiers.length;i++){const midpoint=(exportsObject.leverageTiers[i-1]+exportsObject.leverageTiers[i])/2;leverageInputs.push(midpoint,midpoint-Number.EPSILON*midpoint,midpoint+Number.EPSILON*midpoint);}
for(let i=-100;i<=250;i++)leverageInputs.push(i/4);
const leverageTiers=leverageInputs.map(requested=>({requested,expected:exportsObject.snapToLeverageTier(requested)}));
const tickDelays=[NaN,Infinity,-Infinity,-0,0,-1,Number.MIN_VALUE,1,1999,2000,2001,Number.MAX_VALUE].map(interval=>{tickRuntime.interval=interval;tickRuntime.calls=0;const expected=exportsObject.getInitialTickDelay();assert.equal(tickRuntime.calls,1,'getInitialTickDelay reads interval exactly once');return {interval,expected};});
const marginInputs=[...inputs,'0','1','00','01','cross','CROSS','Cross','isolated','ISOLATED',' isolated ','cross\uFEFF','\uFEFFcross','\u0085isolated',['cross'],['isolated'],[['cross']],['0'],['1'],[0],[1],NaN,Infinity,-Infinity];
const marginModes=marginInputs.map(value=>({value,expected:outcome(()=>exportsObject.parseMarginMode(value))}));
const marginToTypes=[undefined,-0,0,1].map(value=>({value,expected:exportsObject.marginModeToMarginType(value)}));
const typesToMargin=[undefined,...Object.values(exportsObject.MarginType)].map(value=>({value,expected:exportsObject.marginTypeToMarginMode(value)}));
const accountIndices=[...inputs,NaN,Infinity,-Infinity,1e20,1e21,1e22,'1e20','0b10','0o10','-0x10','１２', '9'.repeat(400),0n,-1n,10n**400n,[],[null],['12'],['1','2']].map(input=>({input,expected:exportsObject.parseAccountIndex(input)?.toString()??null}));
const lowerAddress='0x'+'a'.repeat(40),upperAddress='0x'+'F'.repeat(40);
const addressInputs=[undefined,'',' ',lowerAddress,upperAddress,'0X'+'a'.repeat(40),'0x'+'a'.repeat(39),'0x'+'a'.repeat(41),'0x'+'g'.repeat(40),'0x'+'０'.repeat(40),'0x'+'a'.repeat(20)+'\n'+'a'.repeat(20)];
for(const address of [lowerAddress,upperAddress,'0X'+'a'.repeat(40)])for(const prefix of ['','prefix','😀','\uFEFF','\u0085','\n',upperAddress+' '])for(const suffix of ['','suffix','😀','\uFEFF','\u0085','\n'])addressInputs.push(prefix+address+suffix);
const addressCandidates=addressInputs.map(input=>({input,expected:exportsObject.extractL1AddressCandidate(input)??null}));
const reduceSides=Object.values(exportsObject.OrderSide).map(side=>({side,expected:exportsObject.getReduceOnlyClosingPositionSide(side)}));
const reduceMessages=[];
for(const side of Object.values(exportsObject.OrderSide))for(const symbol of ['BTC/USDC','','😀\uFEFF\u0085','x\ny'])reduceMessages.push({side,symbol,expected:exportsObject.buildNoPositionReduceOnlyMessage(symbol,side)});
const positionChecks=[];
const position=(symbol,side,contracts)=>({id:'p',symbol,side,contracts,entryPrice:1,notional:1,leverage:1,unrealizedPnl:0,liquidationPrice:0});
for(const side of Object.values(exportsObject.OrderSide))for(const symbol of ['BTC/USDC','','😀'])for(const positionSide of Object.values(exportsObject.PositionSide))for(const positionSymbol of [symbol,'mismatch'])for(const contracts of [NaN,Infinity,-Infinity,-1,-0,0,Number.MIN_VALUE,1]){
 const candidate=position(positionSymbol,positionSide,contracts);
 for(const positions of [[],[candidate],[position('unrelated',positionSide,3),candidate],[candidate,position(symbol,positionSide,0)]])positionChecks.push({side,symbol,positions,expected:exportsObject.hasOpenReduceOnlyTargetPosition(symbol,side,positions)});
}
const minimumValues=[NaN,Infinity,-Infinity,-1,-0,0,Number.MIN_VALUE,0.1,1,1e-7,1e21,Number.MAX_VALUE];
const minimumReasons=[];
for(const amount of minimumValues)for(const executionPrice of minimumValues)for(const minBaseAmount of minimumValues)for(const minQuoteAmount of [0,1,NaN,Infinity])minimumReasons.push({amount,executionPrice,minBaseAmount,minQuoteAmount,expected:exportsObject.getOrderMinimumViolationReasons(amount,executionPrice,minBaseAmount,minQuoteAmount)??null});
const rateValues=[undefined,null,false,true,0,-0,405,429,404,'405','429','405abc','0x1ad','0x195','\uFEFF429\uFEFF','\u0085429','',[],[429],['405'],{}, {toString:null},[{toString:0}],1e400];
const rateCases=[];
for(const value of rateValues)for(const error of [{statusCode:value},{response:{status:value}},{code:value},{response:{data:{code:value}}},{statusCode:value,response:{status:429}},{statusCode:null,response:{status:value}},{code:value,response:{data:{code:'405'}}},{statusCode:429,code:value}])for(const message of ['','RATE LIMIT','Too Many Requests','too many websocket messages','VOLUME QUOTA','volume\uFEFFquota','rate\u0085limit','rate limiter'])rateCases.push({error,message,expected:outcome(()=>exportsObject.isRateLimitOrderError(error,message))});
for(const outer of rateValues)for(const inner of rateValues)rateCases.push({error:{statusCode:outer,response:{status:inner}},message:'',expected:outcome(()=>exportsObject.isRateLimitOrderError({statusCode:outer,response:{status:inner}},''))});
const stopSlippageCases=[];
for(const explicit of [undefined,NaN,Infinity,-Infinity,-1,-0,0,0.001,1,49.999,50,50.001,1e21])for(const fallback of [...inputs,NaN,Infinity,-Infinity,'50.1tail', {toString:null},'0x10']) {
 stopOptions.extra={lighterStopSlippagePct:fallback};
 stopSlippageCases.push({explicit,fallback,expected:outcome(()=>exportsObject.resolveStopSlippagePct({maxSlippagePct:explicit}))});
}
const configuredMargins=[];
const addConfigured=info=>{const market={id:'0',symbol:'BTC/USDC',base:'BTC',quote:'USDC',active:true,precision:{amount:0.01,price:0.1},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}},info};configuredMargins.push({market,expected:outcome(()=>exportsObject.getMarketConfiguredMarginMode(market))});};
for(const value of marginInputs)for(let index=0;index<4;index++)for(const fill of [undefined,null,0,1,'cross','bad',{toString:null}]){const values=Array.from({length:4},(_,i)=>i===index?value:fill);addConfigured({market_config:{market_margin_mode:values[0]},marketConfig:{marketMarginMode:values[1]},market_margin_mode:values[2],marketMarginMode:values[3]});}
for(const info of [undefined,null,false,[],{},'text'])addConfigured(info);
configuredMargins.push({expected:outcome(()=>exportsObject.getMarketConfiguredMarginMode(undefined))});
const marginMessages=[];
for(const {market} of configuredMargins)for(const symbol of ['BTC/USDC','😀\uFEFF\u0085','']){marketStore.marketsBySymbol={[symbol]:market};marginMessages.push({market,symbol,expected:outcome(()=>exportsObject.buildInvalidMarginModeOrderMessage(symbol))});}
const timeframeAlias=domain.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='Timeframe');
assert.ok(timeframeAlias&&ts.isUnionTypeNode(timeframeAlias.type),'live Timeframe aliases');
const fallbackKeys=[];
for(const node of timeframeAlias.type.types){assert.ok(ts.isLiteralTypeNode(node)&&ts.isStringLiteral(node.literal),'Timeframe literal');for(const symbol of ['',':','BTC/USDC','a:b','😀\uFEFF\u0085','line\nnext']){const interval=node.literal.text;fallbackKeys.push({symbol,interval,expected:exportsObject.getOhlcvFallbackKey({symbol,interval})});}}
const inferredStopLoss=[];
const tickerShape={id:'0',symbol:'BTC/USDC',bid:0,ask:0,last:0,mark:0,index:0,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const addInferred=(order,ticker)=>{marketStore.tickersBySymbol={[order.symbol]:ticker};inferredStopLoss.push({order,ticker,expected:exportsObject.inferStopLossTypeFromReduceOnlyLimit(order)??null});};
for(const type of Object.values(exportsObject.OrderType))for(const side of Object.values(exportsObject.OrderSide))for(const reduceOnly of [false,true])for(const price of [NaN,Infinity,-Infinity,-0,0,-1,Number.MIN_VALUE,99,100,101,Number.MAX_VALUE]){
 const order={id:'1',symbol:'BTC/USDC',type,side,reduceOnly,price,amount:1,filled:0,remaining:1,lastUpdate:0,status:'open'};
 for(const ticker of [undefined,{...tickerShape,mark:100,last:200,index:300},{...tickerShape,mark:NaN,last:100,index:200},{...tickerShape,mark:0,last:Infinity,index:100},tickerShape])addInferred(order,ticker);
}
for(const symbol of ['constructor','__proto__','','😀'])for(const mark of [NaN,Infinity,-Infinity,-0,0,-1,Number.MIN_VALUE,99,100,101,Number.MAX_VALUE])for(const last of [0,100])for(const index of [0,200])for(const side of Object.values(exportsObject.OrderSide))addInferred({id:'1',symbol,type:exportsObject.OrderType.Limit,side,reduceOnly:true,price:100,amount:1,filled:0,remaining:1,lastUpdate:0,status:'open'},{...tickerShape,mark,last,index});
const referencePrices=[];
const priceValues=[NaN,Infinity,-Infinity,-0,0,-1,Number.MIN_VALUE,99,100,101,Number.MAX_VALUE];
for(const symbol of ['BTC/USDC','😀\uFEFF\u0085',''])for(const side of Object.values(exportsObject.OrderSide)) {
 const add=ticker=>{marketStore.tickersBySymbol={[symbol]:ticker};referencePrices.push({symbol,side,ticker,expected:outcome(()=>exportsObject.resolveMarketOrderReferencePrice(symbol,side))});};
 add(undefined);
 for(const bid of priceValues)for(const ask of priceValues)for(const prices of [{mark:100,last:200,index:300},{mark:0,last:200,index:300},{mark:NaN,last:Infinity,index:300},{mark:0,last:0,index:0}])add({...tickerShape,bid,ask,...prices});
}
const referenceStringBoundaries=['\uD800','\uDC00','\uD80012'].map(symbol=>{
 marketStore.tickersBySymbol={};
 try{exportsObject.resolveMarketOrderReferencePrice(symbol,exportsObject.OrderSide.Buy);throw new Error('expected missing ticker');}
 catch(error){return {inputJson:JSON.stringify(symbol),errorName:error.name,messageUnits:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))};}
});
process.stdout.write(JSON.stringify({referencePrices,referenceStringBoundaries,inferredStopLoss,fallbackKeys,configuredMargins,marginMessages,stopSlippageCases,rateCases,reduceSides,reduceMessages,positionChecks,minimumReasons,accountIndices,addressCandidates,cases,fractions,markets,leverages,wasmIssues,orderErrors,priceCandidates,batches,leverageTiers,tickDelays,marginModes,marginToTypes,typesToMargin},(_key,value)=>typeof value==='bigint'?{$bigint:value.toString()}:typeof value==='number'&&(!Number.isFinite(value)||Object.is(value,-0))?{$number:Object.is(value,-0)?'-0':String(value)}:value));
