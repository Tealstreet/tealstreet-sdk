// The actual class members run with injected store/log state and actual shared TS arithmetic.
const fs=require('node:fs');
const fieldIdentity=process.argv.includes('--field-identity');
const path=require('node:path');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
function source(file) {
 let text=fs.readFileSync(path.join(root,file),'utf8');
 if(process.env.BITGET_ORDER_MUTATION && file.endsWith('/bitget.exchange.ts')) {
  const mode=process.env.BITGET_ORDER_MUTATION;
  assert.ok(['strategy-trigger-swap','own-type-only'].includes(mode),'known order mutation');
  const from=mode==='strategy-trigger-swap'?'opts.tpTriggerType = order.tpTriggerType':'type: ORDER_TYPE[o.orderType || o.ordType || o.planType],';
  const to=mode==='strategy-trigger-swap'?'opts.tpTriggerType = order.slTriggerType':'type: Object.hasOwn(ORDER_TYPE, o.orderType || o.ordType || o.planType) ? ORDER_TYPE[o.orderType || o.ordType || o.planType] : undefined,';
  assert.equal(text.split(from).length,2,'order mutation source anchor');
  text=text.replace(from,to);
 }
 if(process.env.BITGET_STREAM_MUTATION && file.endsWith('/bitget.ws-private.ts')) {
  assert.equal(process.env.BITGET_STREAM_MUTATION,'ambiguous-fill','known stream mutation');
  const from="    'full_fill',";
  assert.equal(text.split(from).length,2,'terminal fill source anchor');
  text=text.replace(from,from+"\n    'success',");
 }
 if(process.env.BITGET_CANCEL_MUTATION && file.endsWith('/bitget.exchange.ts')) {
  assert.equal(process.env.BITGET_CANCEL_MUTATION,'own-only','known cancellation mutation');
  const from='if (planTypeMap[rawPlanType]) return planTypeMap[rawPlanType];';
  assert.equal(text.split(from).length,2,'cancellation source anchor');
  text=text.replace(from,'if (Object.hasOwn(planTypeMap, rawPlanType) && planTypeMap[rawPlanType]) return planTypeMap[rawPlanType];');
 }
 if(process.env.BITGET_FOLD_MUTATION && file.endsWith('/bitget.ws-private.ts')) {
  assert.equal(process.env.BITGET_FOLD_MUTATION,'dedupe-rest','known position fold mutation');
  const from='for (const restPosition of restPositions) {';
  assert.equal(text.split(from).length,2,'position fold source anchor');
  text=text.replace(from,from+' if (usedIds.has(restPosition.id)) continue;');
 }
 return ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
}
function evaluate(text,bindings={}) { const exports={};new Function('exports',...Object.keys(bindings),ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exports,...Object.values(bindings));return exports; }
const domain=source('packages/safe-cex/src/types.ts');
const enums=evaluate(domain.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(domain)).join('\n'));
const math=source('packages/safe-cex/src/utils/safe-math.ts');
const invalid=source('packages/math/src/isNotNumber.ts');
const isNotNumber=evaluate(invalid.getText()).isNotNumber;
const arithmetic=evaluate(math.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(math)).join('\n'),{isNotNumber});
const venue=source('packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const successNode=venue.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(venue)==='isBitgetSuccessCode'));
assert.ok(successNode,'isBitgetSuccessCode source declaration');
const {isBitgetSuccessCode}=evaluate('export '+successNode.getText(venue));
const klass=venue.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetExchange');
const protocol=source('packages/safe-cex/src/exchanges/bitget/bitget.types.ts');
const tables=evaluate(protocol.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(protocol)).join('\n'),enums);
const names=['getCancelPlanType','mapOrder','buildUtaStrategyPlaceOptsFromOrder','getOrderSideV2','getOrderSideUta','getUtaStrategyOrderSide','getUtaStrategyTriggerBy','getAlgoOrderSide','isVirtualTPSLOrder','isAlgoOrder','planTypeComparableAmount','floorToLot','selectPlanTypeAndAmount','toFiniteNumber','firstFiniteNumber','normalizeBitgetSymbol','normalizeBitgetPositionMode','normalizeBitgetMarginType','normalizeBitgetPositionSide','resolveLiquidationPrice','_resolveMarket','mapUtaPosition','mapPosition','calculateUpnlFromLastPrice','calculateAccountUpnl'];
const members=klass.members.filter(n=>names.includes(n.name?.getText(venue)));
assert.deepEqual(members.map(n=>n.name.getText(venue)).sort(),[...names].sort(),'Bitget pure order member inventory');
const idSource=source('packages/safe-cex/src/utils/createPositionId.ts');
const {createPositionId}=evaluate(idSource.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(idSource)).join('\n'),enums);
let clockCalls=0;
const {Pure}=evaluate('export class Pure { store:any; emitAndLogError:any;\n'+members.map(n=>n.getText(venue)).join('\n')+'\n}',{...enums,...tables,...arithmetic,createPositionId,virtualClock:{getCurrentTimeMs:()=>{clockCalls++;return 1700000000123;}}});
const inverse=source('packages/safe-cex/src/exchanges/bitget/bitgetinverse.exchange.ts');
const inverseClass=inverse.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetInverseExchange');
const inverseNames=['getInverseAlgoOrderSide','generateUuid','getMarginCoinForSymbol'];
const inverseMembers=inverseClass.members.filter(n=>inverseNames.includes(n.name?.getText(inverse)));
assert.deepEqual(inverseMembers.map(n=>n.name.getText(inverse)).sort(),[...inverseNames].sort(),'Bitget inverse pure member inventory');
let draws=[],drawIndex=0;
const {Inverse}=evaluate('export class Inverse {\n'+inverseMembers.map(n=>n.getText(inverse)).join('\n')+'\n}',{...enums,Math:{random:()=>{assert.ok(drawIndex<draws.length,'UUID random stream exhausted');return draws[drawIndex++];}}});
const inverseInstance=new Inverse();
const instance=new Pure();
const encode=value=> typeof value==='number' ? {$number:Object.is(value,-0)?'-0':String(value)} : Array.isArray(value) ? value.map(encode) : value&&typeof value==='object' ? Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])) : value;
const capture=fn=>{try{return {value:encode(fn())};}catch(error){return {error:{name:error.name,message:error.message}};}};
const sideCases=[];
for(const side of Object.values(enums.OrderSide))for(const mode of Object.values(enums.PositionMode))for(const reduceOnly of [undefined,false,true])for(const type of Object.values(enums.OrderType)){
 const opts={symbol:'BTCUSDT',side,reduceOnly,type,amount:1};
 sideCases.push({side,mode,reduceOnly,type,v2:instance.getOrderSideV2(opts,mode),uta:instance.getOrderSideUta(opts,mode),strategy:instance.getUtaStrategyOrderSide(opts,mode),algo:capture(()=>instance.getAlgoOrderSide(opts,mode)),isAlgo:instance.isAlgoOrder(opts),inverseAlgo:capture(()=>inverseInstance.getInverseAlgoOrderSide(opts,mode))});
}
const triggers=[undefined,...Object.values(enums.OrderStopTriggerType)].map(trigger=>({trigger,value:instance.getUtaStrategyTriggerBy(trigger)}));
const ids=['','123','__stop_loss','x__take_profit','x__stop_loss\n','x__stop_loss_tail','x__STOP_LOSS','\uFEFF__stop_loss','\u0085__take_profit','📈__take_profit'].map(id=>({id,value:instance.isVirtualTPSLOrder({id})}));
const amounts=[0,-0,0.29,0.4,1,30,30.34,30.6,31,-1,NaN,Infinity,-Infinity];
const steps=[undefined,0,-0,NaN,Infinity,-1,1,0.01,0.1];
const roundCases=[];
for(const amount of amounts)for(const step of steps){instance.store={marketsBySymbol:{BTCUSDT:{precision:{amount:step}}}};roundCases.push({amount:encode(amount),step:step===undefined?undefined:encode(step),comparable:encode(instance.planTypeComparableAmount({symbol:'BTCUSDT',amount})),floor:encode(instance.floorToLot('BTCUSDT',amount))});}
const position=(symbol,side,contracts)=>({id:symbol+side,symbol,side,contracts,entryPrice:1,notional:1,leverage:1,unrealizedPnl:0,liquidationPrice:0});
const positionSets=[[],[position('BTCUSDT','long',30.6)],[position('BTCUSDT','short',30)],[position('BTCUSDT','long',30),position('BTCUSDT','short',20)],[position('OTHER','long',100)],[position('BTCUSDT','long',0.4),position('BTCUSDT','short',0.4)],[position('BTCUSDT','long',0.4),position('BTCUSDT','long',50)],[position('BTCUSDT','short',Infinity)]];
const sizingCases=[];
for(const side of Object.values(enums.OrderSide))for(const type of [enums.OrderType.StopLoss,enums.OrderType.TakeProfit,enums.OrderType.TrailingStopLoss,enums.OrderType.Market])for(const [i,amount]of amounts.entries())for(const [j,positions]of positionSets.entries()){
 const step=steps[(i+j)%steps.length];const opts={symbol:'BTCUSDT',side,type,amount};const errors=[];
 instance.store={marketsBySymbol:{BTCUSDT:{precision:{amount:step}}}};instance.emitAndLogError=m=>errors.push(m);
 const result=capture(()=>instance.selectPlanTypeAndAmount(opts,positions));
 sizingCases.push({opts:encode(opts),positions:encode(positions),step:step===undefined?undefined:encode(step),result,errors});
}
const utf16Ids=['\uD800__stop_loss','\uDC00__take_profit'].map(id=>({json:JSON.stringify(id),value:instance.isVirtualTPSLOrder({id})}));
const utf16Sizing=['\uD800','\uDC00'].map(symbol=>{
 const opts={symbol,type:enums.OrderType.StopLoss,side:enums.OrderSide.Sell,amount:1};const errors=[];
 instance.store={marketsBySymbol:{}};instance.emitAndLogError=m=>errors.push(m);
 const result=capture(()=>instance.selectPlanTypeAndAmount(opts,[]));
 return {json:JSON.stringify(opts),result,errorsJson:JSON.stringify(errors)};
});
const uuidCases=[];
for(const drawValues of [Array(31).fill(0),Array(31).fill(1-Number.EPSILON/2),Array(31).fill(0.5),Array.from({length:31},(_,i)=>(i%16)/16),Array.from({length:31},(_,i)=>(15-i%16)/16)]) {
 draws=drawValues;drawIndex=0;const value=inverseInstance.generateUuid();uuidCases.push({draws,value,consumed:drawIndex});
}
const pnlCases=[];
const pnlNumbers=[0,-0,1,-2,0.29,1e308,NaN,Infinity,-Infinity];
function capturePnl(positions,last,apiUpnl){
 const calls=[];
 instance.store={positions,tickersBySymbol:new Proxy({}, {get:(_,symbol)=>{calls.push(symbol);return last===undefined?undefined:{last};}})};
 const upnl=encode(instance.calculateUpnlFromLastPrice(positions));const upnlCalls=[...calls];calls.length=0;
 const account=encode(instance.calculateAccountUpnl(apiUpnl));
 return {positions:encode(positions),last:last===undefined?undefined:encode(last),apiUpnl:encode(apiUpnl),upnl,upnlCalls,account,accountCalls:[...calls]};
}
for(const side of Object.values(enums.PositionSide))for(const [i,contracts]of pnlNumbers.entries())for(const entryPrice of pnlNumbers)for(const last of [undefined,0,5,1e308,NaN,Infinity]) {
 const p={...position('BTCUSDT',side,contracts),entryPrice,unrealizedPnl:pnlNumbers[(i+2)%pnlNumbers.length]};
 pnlCases.push(capturePnl([p],last,pnlNumbers[(i+3)%pnlNumbers.length]));
}
for(const positions of [[],[position('BTCUSDT','long',0),position('OTHER','short',2)],[{...position('BTCUSDT','long',1),unrealizedPnl:1e308},{...position('OTHER','short',1),unrealizedPnl:1e308}],[position('BTCUSDT','long',-2),position('BTCUSDT','long',3),position('BTCUSDT','short',1)]])pnlCases.push(capturePnl(positions,undefined,17));
const utf16Pnl=['\uD800','\uDC00'].map(symbol=>{
 const positions=[position(symbol,'long',2)];const captured=capturePnl(positions,5,0);
 return {json:JSON.stringify(positions),upnl:captured.upnl,account:captured.account,callsJson:JSON.stringify(captured.upnlCalls)};
});
const finiteInputs=[undefined,null,true,false,{}, {toString:0},[],['12'],0,-0,NaN,Infinity,-Infinity,1e308,1e21,'',' ','\uFEFF','\u0085','\uFEFF12.5tail','\u008512.5','12abc','0x10','Infinity','1e309','-0','3e-324','2e-324','0.000001'];
const finiteCases=[];
for(const input of finiteInputs)for(const fallback of [0,-0,17,NaN,Infinity,-Infinity])finiteCases.push({input:encode(input),fallback:encode(fallback),value:encode(instance.toFiniteNumber(input,fallback))});
const firstFiniteCases=[];
for(const input of finiteInputs)for(const next of finiteInputs)for(const fallback of [0,-0,17,NaN]){const values=[undefined,input,next];firstFiniteCases.push({values:encode(values),fallback:encode(fallback),value:encode(instance.firstFiniteNumber(values,fallback))});}
firstFiniteCases.push({values:[],fallback:encode(17),value:encode(instance.firstFiniteNumber([],17))});
const utf16Finite=['\uD80012','12\uDC00'].map(input=>({json:JSON.stringify(input),value:encode(instance.toFiniteNumber(input,17)),first:encode(instance.firstFiniteNumber([input],17))}));
const normalizationValues=[undefined,null,'','hedge_mode','double_hold','HEDGE_MODE','hedge_mode\uFEFF','\uFEFFhedge_mode','\u0085hedge_mode','cross','crossed','CROSS','cross ','short','sell','SELL','short\uFEFF',true,false,0,1,[],['short'],['hedge_mode'],['cross'],{}, {toString:0},[{toString:0}],'İ','Σ','𐐀'];
const normalizeCases=[];
for(const value of normalizationValues){
 normalizeCases.push({name:'margin',input:encode(value),result:capture(()=>instance.normalizeBitgetMarginType(value))});
 normalizeCases.push({name:'mode',input:encode(value),result:capture(()=>instance.normalizeBitgetPositionMode(value))});
 normalizeCases.push({name:'side',input:encode(value),result:capture(()=>instance.normalizeBitgetPositionSide(value))});
 for(const fallback of normalizationValues){
  const mode={posMode:value,holdMode:fallback};const side={holdSide:value,posSide:fallback,side:'sell'};
  normalizeCases.push({name:'mode',input:encode(mode),result:capture(()=>instance.normalizeBitgetPositionMode(mode))});
  normalizeCases.push({name:'side',input:encode(side),result:capture(()=>instance.normalizeBitgetPositionSide(side))});
 }
}
const utf16Normalize=['\uD800','\uDC00'].map(value=>({json:JSON.stringify(value),mode:instance.normalizeBitgetPositionMode({posMode:value}),margin:instance.normalizeBitgetMarginType(value),side:instance.normalizeBitgetPositionSide({holdSide:value})}));
const coercionInputs=[undefined,null,'',' ','\uFEFF','\u0085','\uFEFF0tail','\u00850','12abc','0x10','0b10','Infinity','-Infinity','1e309','-0',0,-0,1e21,1e-7,false,true,[],[null],[12.5],[1,2],{}, {toString:0},[{toString:0}]];
const successCases=coercionInputs.map(input=>({input:encode(input),result:capture(()=>isBitgetSuccessCode(input))}));
const liquidationCases=[];
for(const input of coercionInputs)for(const stored of [undefined,0,-0,17,-5,NaN,Infinity,-Infinity])for(const target of ['BTCUSDTlong','missing']){
 const positions=stored===undefined?[]:[{...position('OTHER','short',2),liquidationPrice:99},{...position('BTCUSDT','long',2),liquidationPrice:stored},{...position('BTCUSDT','long',2),liquidationPrice:123}];
 instance.store={positions};liquidationCases.push({input:encode(input),target,positions:encode(positions),result:capture(()=>instance.resolveLiquidationPrice(target,input))});
}
const utf16Coercion=['\uD800','0\uDC00'].map(input=>{instance.store={positions:[]};return {json:JSON.stringify(input),success:isBitgetSuccessCode(input),liquidation:encode(instance.resolveLiquidationPrice('absent',input))};});
const storePositionCases=JSON.parse(require('node:child_process').execFileSync(process.execPath,[path.join(__dirname,'store-position-capture.mjs')],{encoding:'utf8'}));
function profileClass(ast,name,base){
 const node=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===name);
 const getters=node.members.filter(n=>ts.isGetAccessor(n)&&['apiProductType','apiMarginCoin','apiUtaCategory','usesUtaRouting','suppressBitgetValidationErrorEmission'].includes(n.name.getText(ast)));
 assert.deepEqual(getters.map(n=>n.name.getText(ast)).sort(),(base?['apiProductType','apiMarginCoin']:[ 'apiProductType','apiMarginCoin','apiUtaCategory','usesUtaRouting','suppressBitgetValidationErrorEmission']).sort(),name+' profile getter inventory');
 return evaluate('export class Profile '+(base?'extends Base ':'')+'{ options:any; '+getters.map(n=>n.getText(ast)).join('\n')+'}',base?{Base:base}:{}).Profile;
}
const MainProfile=profileClass(venue,'BitgetExchange');
const UsdcProfile=profileClass(source('packages/safe-cex/src/exchanges/bitget/bitgetusdc.exchange.ts'),'BitgetUSDCExchange',MainProfile);
const InverseProfile=profileClass(inverse,'BitgetInverseExchange',MainProfile);
if(process.argv.includes('--unicode-profile')){
 let product='';for(let code=0;code<=0x10ffff;code++){if(code<0xd800||code>0xdfff)product+=String.fromCodePoint(code);}
 const instance=new MainProfile();Object.defineProperty(instance,'apiProductType',{value:product});
 fs.writeFileSync(1,instance.apiUtaCategory);process.exit(0);
}
const profileCases=[];
for(const [name,Profile] of [['main',MainProfile],['usdc',UsdcProfile],['inverse',InverseProfile]])for(const testnet of [undefined,false,true]){
 const instance=new Profile();instance.options={testnet};profileCases.push({name,testnet,product:instance.apiProductType,margin:instance.apiMarginCoin,category:instance.apiUtaCategory});
}
const stateGetterCases=[];
for(const isUta of [false,true])for(const supportsUtaRouting of [false,true])for(const testnet of [undefined,false,true])for(const suppressed of [false,true]){
 const p=new MainProfile();p.isUta=isUta;p.supportsUtaRouting=supportsUtaRouting;p.options={testnet};p._suppressBitgetValidationErrorEmission=suppressed;
 stateGetterCases.push({isUta,supportsUtaRouting,testnet,suppressed,routing:p.usesUtaRouting,suppression:p.suppressBitgetValidationErrorEmission});
}
const categoryCases=['','usdt-futures','susdt-futures','usdc-futures','susdc-futures','coin-futures','scoin-futures','İ','ſ','ΟΣ','𐐨','ß','\uFEFFusdt-futures','\u0085usdt-futures'].map(product=>{const instance=new MainProfile();Object.defineProperty(instance,'apiProductType',{value:product});return {product,value:instance.apiUtaCategory};});
const utf16Category=['\uD800','x\uDC00'].map(product=>{const instance=new MainProfile();Object.defineProperty(instance,'apiProductType',{value:product});return {json:JSON.stringify(product),outputJson:JSON.stringify(instance.apiUtaCategory)};});
const symbolCases=[];
const symbolInputs=[undefined,null,false,0,-0,1e21,[],['BTC_UMCBL'],{}, {toString:3}, '', 'BTC', 'BTC_UMCBL', 'BTC_SUMCBL','BTC_DMCBL','BTC_CMCBL','btc_umcbl','BTC_umcbl_UMCBL','BTC_UMCBL_usdt-futures','BTC_usdt-futures_UMCBL','BTC_USDT-FUTURES','BTC_SUSDT-FUTURES','BTC_USDC-FUTURES','BTC_SUSDC-FUTURES','BTC_COIN-FUTURES','BTC_SCOIN-FUTURES',...['\n','\r','\u2028','\u2029','\uFEFF','\u0085'].map(s=>'BTC_UMCBL'+s),'\uFEFFBTC_UMCBL','BTC_ſUMCBL','📈_CMCBL'];
for(const [profile,Profile] of [['main',MainProfile],['usdc',UsdcProfile],['inverse',InverseProfile]])for(const testnet of [false,true])for(const input of symbolInputs)for(const resolution of ['missing','null','empty','mapped','throw']){
 const receiver=new Pure();const p=new Profile();p.options={testnet};receiver.apiProductType=p.apiProductType;
 const calls=[];receiver._resolveMarket=s=>{calls.push(s);if(resolution==='throw')throw new TypeError('resolver failure');return resolution==='missing'?undefined:resolution==='null'?{symbol:null}:{symbol:resolution==='empty'?'':'MAPPED'};};
 const result=capture(()=>receiver.normalizeBitgetSymbol(input));symbolCases.push({profile,testnet,input:encode(input),resolution,calls,result});
}
const utf16Symbol=['\uD800_UMCBL','\uDC00'].map(input=>{const receiver=new Pure();receiver.apiProductType='usdt-futures';receiver._resolveMarket=()=>undefined;return {json:JSON.stringify(input),outputJson:JSON.stringify(receiver.normalizeBitgetSymbol(input))};});
const market=(id,symbol,base,quote)=>({id,symbol,base,quote,active:true,precision:{amount:1,price:1},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}});
const marketSets=[[],[market('btc','BTCUSDT','BTC','USDT'),market('eth','ETHUSD','ETH','USD')],[market('first','venue1','BTC','USDC'),market('second','venue2','BTC','USDC'),market('busd','venue3','BTCB','USDT')],[market('old','BTCUSDT','BTC','USDT'),market('new','BTCUSDT','BTC','USDT')],[market('own','constructor','OWN','BTC'),market('empty','EMPTY','','USDT')],[market('proto-fallback','venue-constructor','CONSTRUCTOR','USDT')]];
const marketSymbols=['','BTCUSDT','btcusdt','BTC','BTC-PERP','BTC_PERP','btc-swap','BTCPERPSWAP','BTCSWAPPERP','BTCBUSD','ETHUSD','BTCUSD','BTC/USDT','BTCUSD\n','BTCUSD\r','BTCUSD\u2028','BTCUSD\u2029','BTCUSD\uFEFF','\uFEFFBTC','\u0085BTC','EMPTY','İ','ß',...Object.getOwnPropertyNames(Object.prototype)];
const marketCases=[];
for(const markets of marketSets)for(const symbol of marketSymbols)for(const marginCoin of ['USDT','USDC','USD','BTC']){
 const raw={};for(const m of markets)raw[m.symbol]=m;
 const calls=[];const map=new Proxy(raw,{get:(o,k)=>{calls.push(k);return o[k];}});
 instance.store={markets,marketsBySymbol:map};instance.apiMarginCoin=marginCoin;
 const found=instance._resolveMarket(symbol);const inherited=found!==undefined&&!markets.includes(found);
 const result=inherited?{inheritedType:typeof found,inheritedString:String(found)}:found===undefined?null:{id:found.id};
 const resolveCalls=[...calls];calls.length=0;inverseInstance.store={marketsBySymbol:map};
 const coin=inverseInstance.getMarginCoinForSymbol(symbol);
 marketCases.push({markets,symbol,marginCoin,result,resolveCalls,coin:coin===undefined?{$undefined:true}:coin,coinCalls:[...calls]});
}
const utf16Market=['\uD800USD','\uDC00'].map(symbol=>{instance.store={markets:[],marketsBySymbol:{}};instance.apiMarginCoin='USDT';inverseInstance.store=instance.store;return {json:JSON.stringify(symbol),resolved:instance._resolveMarket(symbol)??null,coinJson:JSON.stringify(inverseInstance.getMarginCoinForSymbol(symbol))};});
const positionInputs=[undefined,null,false,0,'text',[],{}, {symbol:'BTC_UMCBL',total:'2',size:'3',avgPrice:'5',openPriceAvg:'7',positionBalance:'11',marginSize:'13',unrealisedPnl:'17',unrealizedPL:'19',curRealisedPnl:'23',totalFunding:'29',openFeeTotal:'-31',closeFeeTotal:'-37',deductedFee:'41',achievedProfits:'1000',totalFee:'2000'}];
const positionFields=['total','size','markPrice','marketPrice','avgPrice','openPriceAvg','openAvgPrice','avgPx','positionBalance','marginSize','leverage','lever','unrealisedPnl','unrealizedPL','upl','curRealisedPnl','totalFunding','openFeeTotal','closeFeeTotal','deductedFee','pos','openTotalPos','available','averageOpenPrice','margin','achievedProfits','totalFee'];
for(const field of positionFields)for(const value of [null,'','junk','0x10','12tail','\uFEFF12.5','\u008512.5','-0',-0,0,3,'1e308','Infinity',[],{}, {toString:0}])positionInputs.push({symbol:'BTC_USDT-FUTURES',size:'2',openPriceAvg:'7',[field]:value});
for(const field of ['symbol','instId','posMode','holdMode','holdSide','posSide','side','marginMode','liquidationPrice','liqPrice','liqPx'])for(const value of [null,'','short','hedge_mode','crossed','-0','Infinity',[],['BTC'],{toString:0}])positionInputs.push({symbol:'BTC',size:'2',[field]:value});
positionInputs.push({symbol:'BTC',total:'1e308',markPrice:'1e308',curRealisedPnl:'1e308',totalFunding:'1e308',openFeeTotal:'-1e308',closeFeeTotal:'-1e308'}, {symbol:'BTC',positionBalance:'0',marginSize:'5',posMode:'',holdMode:'hedge_mode',holdSide:'',posSide:'short'});
const utaPositionCases=[];const classicPositionCases=[];
for(const input of positionInputs)for(const stored of [undefined,17,Infinity]){
 const receiver=new Pure();receiver.apiProductType='usdt-futures';const calls=[];receiver._resolveMarket=s=>{calls.push(s);return s==='BTC'?{symbol:'BTCUSDT'}:undefined;};
 receiver.store={positions:stored===undefined?[]:[{id:'BTCUSDT',liquidationPrice:stored}]};clockCalls=0;
 const result=capture(()=>receiver.mapUtaPosition(input));utaPositionCases.push({input:encode(input),stored:stored===undefined?undefined:encode(stored),result,calls:[...calls],clockCalls});
 calls.length=0;clockCalls=0;const classic=capture(()=>receiver.mapPosition(input));classicPositionCases.push({input:encode(input),stored:stored===undefined?undefined:encode(stored),result:classic,calls:[...calls],clockCalls});
}
const utf16UtaPosition=['\uD800','\uDC00'].map(symbol=>{const receiver=new Pure();receiver.apiProductType='usdt-futures';receiver.store={positions:[]};receiver._resolveMarket=()=>undefined;return {json:JSON.stringify({symbol}),outputJson:JSON.stringify(receiver.mapUtaPosition({symbol})),classicOutputJson:JSON.stringify(receiver.mapPosition({symbol}))};});
const strategyOptsCases=[];
const strategyBase={id:'order',status:'open',symbol:'BTCUSDT',type:'stop_market',side:'sell',price:12.5,amount:3,filled:1,remaining:2,reduceOnly:true,lastUpdate:1700000000123,triggerType:'index_price',takeProfit:88,stopLoss:77,close:true,info:{keep:false}};
const missing=Symbol('missing');
function strategyCase(input){const output=instance.buildUtaStrategyPlaceOptsFromOrder(input);strategyOptsCases.push({input:encode(input),output:encode(output),ownKeys:Object.keys(output),undefinedKeys:Object.keys(output).filter(key=>output[key]===undefined)});}
for(const type of Object.values(enums.OrderType))for(const side of Object.values(enums.OrderSide))for(const reduceOnly of [false,true])for(const timeInForce of [undefined,...Object.values(enums.OrderTimeInForce)])for(const tp of [missing,undefined,...Object.values(enums.OrderStopTriggerType)])for(const sl of [missing,undefined,...Object.values(enums.OrderStopTriggerType)]){
 const input={...strategyBase,type,side,reduceOnly,timeInForce};
 if(tp!==missing)input.tpTriggerType=tp;if(sl!==missing)input.slTriggerType=sl;
 strategyCase(input);
}
for(const price of [-0,NaN,Infinity,-Infinity,1e21,1e-7])for(const amount of [-0,NaN,Infinity,-Infinity,1e21,1e-7])strategyCase({...strategyBase,price,amount});
const utf16StrategyOpts=['\uD800','x\uDC00'].map(symbol=>{const input={...strategyBase,symbol};return {json:JSON.stringify(input),outputJson:JSON.stringify(instance.buildUtaStrategyPlaceOptsFromOrder(input))};});
const tableLookupInventory=[];
const dynamicMapInventory=[];
for(const file of fs.readdirSync(path.join(root,'packages/safe-cex/src/exchanges/bitget')).filter(file=>file.endsWith('.ts')).sort()){
 const name='packages/safe-cex/src/exchanges/bitget/'+file;
 const tree=ts.createSourceFile(name,fs.readFileSync(path.join(root,name),'utf8'),ts.ScriptTarget.Latest,true);
 function walk(node){
  if(ts.isElementAccessExpression(node)&&((ts.isIdentifier(node.expression)&&(/^[A-Z][A-Z_0-9]*$/.test(node.expression.text)||['planTypeMap','timeframeToSeconds'].includes(node.expression.text)))||(ts.isCallExpression(node.expression)&&node.expression.expression.getText(tree)==='inverseObj')))tableLookupInventory.push({file,expression:node.getText(tree)});
  if(ts.isElementAccessExpression(node)){
   const expression=node.expression.getText(tree);
   if(/(?:this\.(?:parent\.)?store\.(?:marketsBySymbol|marketsById|tickersBySymbol)|this\.(?:_positioningCache|_positioningInflight|messageHandlers|topics))/.test(expression)||['coins','acc'].includes(expression))dynamicMapInventory.push({file,expression:node.getText(tree),operation:ts.isDeleteExpression(node.parent)?'delete':ts.isBinaryExpression(node.parent)&&node.parent.left===node&&node.parent.operatorToken.kind===ts.SyntaxKind.EqualsToken?'write':'read'});
  }
  ts.forEachChild(node,walk);
 }
 walk(tree);
}
assert.equal(tableLookupInventory.length,26,'Bitget named table lookup inventory');
// ts-bug: BitgetExchange.mapOrder — inherited table members are truthy and
// suppress both side fallbacks and the unknown-type strategy recovery branch.
// Keep undefined/function/prototype tags: JSON.stringify alone erases evidence.
function openValue(value) {
 if(value===undefined)return {$undefined:true};
 if(typeof value==='function'){const member=Object.getOwnPropertyNames(Object.prototype).find(key=>Object.getOwnPropertyDescriptor(Object.prototype,key).value===value);return {$function:value.name,$nativeMember:member??null};}
 if(value===Object.prototype)return {$prototype:'Object.prototype'};
 if(typeof value==='number')return encode(value);
 if(Array.isArray(value))return value.map(openValue);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,openValue(v)]));
 return value;
}
const mapOrderReference=[];
const baseOrder={orderId:'x',status:'new',orderType:'limit',side:'buy',symbol:'BTCUSDT',size:'1',price:'1'};
function orderReference(name,patch){
 const input={...baseOrder,...patch};instance.apiProductType='usdt-futures';clockCalls=0;
 let result;try{const output=instance.mapOrder(input);result={value:openValue(output),serialized:JSON.stringify(output)};}catch(error){result={error:{name:error.name,message:error.message}};}
 mapOrderReference.push({name,input:openValue(input),result,clockCalls});
}
orderReference('baseline',{});
orderReference('numeric-id',{orderId:42});
orderReference('missing-id',{orderId:undefined});
orderReference('object-id',{orderId:{wire:true}});
orderReference('unknown-status',{status:'UNKNOWN'});
orderReference('missing-status',{status:undefined});
orderReference('unknown-type',{orderType:'UNKNOWN'});
orderReference('missing-type',{orderType:undefined});
orderReference('unknown-type-strategy',{orderType:'UNKNOWN',stopLoss:'2',takeProfit:'3'});
for(const key of Object.getOwnPropertyNames(Object.prototype)){
 for(const field of ['status','orderType','side','force','planType'])orderReference(`${field}:${key}`,{[field]:key,stopLoss:'2',takeProfit:'3'});
 orderReference(`strategy-side:${key}`,{orderType:'UNKNOWN',side:key,posSide:'short',stopLoss:'2'});
}
const mapOrderCases=[];
function mappedOrder(name,patch,product='usdt-futures'){
 const input={...baseOrder,...patch};instance.apiProductType=product;clockCalls=0;
 let result;try{const output=instance.mapOrder(input);result={value:openValue(output),serialized:JSON.stringify(output)};}catch(error){result={error:{name:error.name,message:error.message}};}
 mapOrderCases.push({name,product,input:openValue(input),result,clockCalls});
}
for(const type of ['limit','market','UNKNOWN','pos_profit','pos_loss'])
 for(const side of ['buy','sell','close_long','constructor','UNKNOWN'])
  for(const mode of ['hedge_mode','one_way_mode',''])
   for(const reduce of ['YES','yes',true,false])
    mappedOrder(`mode:${type}:${side}:${mode}:${reduce}`,{orderType:type,side,posMode:mode,reduceOnly:reduce,stopLoss:'2',takeProfit:'3'});
for(const key of Object.getOwnPropertyNames(Object.prototype))for(const field of ['status','side','force'])
 mappedOrder(`deferred-refusal:${field}:${key}`,{[field]:key,presetStopLossPrice:'2',presetStopSurplusPrice:{toString:1}});
const orderValues=[undefined,null,false,true,0,-0,12,1e21,'','0','-0','NaN','Infinity','-Infinity','0x10','12junk','\ufeff12.5','\u008512.5',[],[2],['12junk'],{}, {toString:1}];
for(const field of ['size','sz','qty','amount','baseVolume','filledQty','accFillSz','cumExecQty','price','px','status','orderType','side','reduceOnly','force','symbol','orderId','triggerPrice','triggerPx','stopLoss','takeProfit'])
 for(let i=0;i<orderValues.length;i++){
  const prefix=['sz','qty','amount'].includes(field)?{size:undefined}:field==='px'?{price:undefined}:field==='triggerPrice'?{planType:'pos_loss',triggerPx:'8'}:field==='triggerPx'?{planType:'pos_loss',triggerPrice:'0'}:{};
  mappedOrder(`coercion:${field}:${i}`,{...prefix,[field]:orderValues[i]});
 }
for(let i=0;i<orderValues.length;i++)mappedOrder(`attached-id:${i}`,{orderId:orderValues[i],stopLoss:'2'});
for(const product of ['usdt-futures','usdc-futures','coin-futures','SUSDT-FUTURES'])mappedOrder(`product:${product}`,{symbol:`BTC_${product.toUpperCase()}_${product.toUpperCase()}`},product);
for(const key of Object.getOwnPropertyNames(Object.prototype))
 for(const planType of ['pos_loss','pos_profit','UNKNOWN'])
  mappedOrder(`overwritten:${key}:${planType}`,{side:key,orderType:key,planType,posMode:'hedge_mode',triggerPrice:'0',triggerPx:'8'});
for(const planType of ['pos_loss','pos_profit','loss_plan','profit_plan','track_plan','UNKNOWN'])
 for(const triggerPrice of ['0','-0','NaN','Infinity','3'])
  for(const triggerType of ['fill_price','mark_price',''])
   mappedOrder(`plan:${planType}:${triggerPrice}:${triggerType}`,{planType,triggerPrice,triggerPx:'4',triggerType});
for(const keys of [['price','px'],['symbol','instId'],['size','sz','qty','amount'],['baseVolume','filledQty','accFillSz','cumExecQty'],['orderId','clientOid','ordId','id'],['orderType','ordType','planType'],['tradeSide','tS','side'],['status','state','planStatus','orderStatus'],['force','timeInForce','tiF'],['presetStopLossPrice','stopLossTriggerPrice','stopLoss'],['presetStopSurplusPrice','stopSurplusTriggerPrice','presetTakeProfitPrice','takeProfit']]){
 const candidates=keys[0]==='orderType'?['limit','market','pos_loss','pos_profit']:keys[0]==='tradeSide'?['buy','sell','close_long']:keys[0]==='status'?['new','filled','cancelled','partially_filled']:keys[0]==='force'?['gtc','fok','ioc']:keys.map((_,i)=>`${i+2}`);
 for(let mask=0;mask<2**keys.length;mask++)mappedOrder(`aliases:${keys[0]}:${mask}`,Object.fromEntries(keys.map((key,i)=>[key,mask&(1<<i)?candidates[i]:null])));
}
for(const slTriggerBy of [undefined,'market','mark','MARKET',null,{},[]])
 for(const tpTriggerBy of [undefined,'market','mark','MARKET',null,{},[]])
  for(const [stopLoss,takeProfit] of [['2','0'],['0','3'],['2','3']])
   mappedOrder(`recovery-trigger:${JSON.stringify([slTriggerBy,tpTriggerBy,stopLoss,takeProfit])}`,{orderType:'UNKNOWN',slTriggerBy,tpTriggerBy,stopLoss,takeProfit});
for(const symbol of ['BTCUSDT_USDT-FUTURES','X_USDT-FUTURES_USDT-FUTURES','x_usdt-futures','\ufeffBTC','\u0085BTC','𐐀','\0'])mappedOrder(`symbol:${JSON.stringify(symbol)}`,{symbol});
const mapOrderUtf16=['\ud800','x\udc00'].map(symbol=>{const input={...baseOrder,symbol};const order=instance.mapOrder(input)[0];return {inputJson:JSON.stringify(input),symbolJson:JSON.stringify(order.symbol)};});
// Observe aliasing before JSON erases the nonfinite assignment. Mutating either
// the gateway input or the returned info is visible through the other reference.
const aliasInput={...baseOrder,label:'before'};
const aliasOrder=instance.mapOrder(aliasInput)[0];
aliasInput.label='after-input';aliasInput.price=NaN;
const afterInput=openValue(aliasOrder.info);
aliasOrder.info.label='after-output';
const mapOrderAlias={sameReference:aliasOrder.info===aliasInput,afterInput,afterOutput:openValue(aliasInput)};
const numericInfoInput={...baseOrder,nested:{amount:NaN}};
const mapOrderNumericInfo={input:openValue(numericInfoInput),output:openValue(instance.mapOrder(numericInfoInput)[0])};
const streamSource=source('packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts');
const streamClass=streamSource.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetPrivateWebsocket');
const streamNames=['getPositionPayloadUpdateTime','mergeRestPositionsWithFreshWsState','normalizeOrderStatus','getRawOrderStatus','normalizeChannel','isTerminalFillStatus','BITGET_TERMINAL_FILL_STATUSES','toFiniteNumber'];
const streamMembers=streamClass.members.filter(n=>streamNames.includes(n.name?.getText(streamSource)));
assert.deepEqual(streamMembers.map(n=>n.name.getText(streamSource)).sort(),[...streamNames].sort(),'private stream pure member inventory');
const Stream=evaluate('export class BitgetPrivateWebsocket {\n'+streamMembers.map(n=>n.getText(streamSource)).join('\n')+'\n}').BitgetPrivateWebsocket;
const stream=new Stream();
const streamInputs=[undefined,null,true,false,0,-0,1,1e21,'','filled','FILLED','full-fill','FULL--FILL','success','executed','triggered','triggered ','partial_fill','order-status_channel','\uFEFFfilled','filled\u0085','İ','ΟΣ','𐐀',[],['FULL-FILL'],{}, {toString:0},[{toString:0}],['A','B'],{valueOf:0}];
const streamNormalizeCases=streamInputs.map(input=>({input:encode(input),status:capture(()=>stream.normalizeOrderStatus(input)),channel:capture(()=>stream.normalizeChannel(input))}));
const streamRawCases=[];
for(const input of streamInputs)for(const key of ['status','state','planStatus','orderStatus']){
 const order={status:null,state:null,planStatus:null,orderStatus:'filled',[key]:input};
 streamRawCases.push({input:encode(order),result:capture(()=>stream.getRawOrderStatus(order))});
}
for(const input of [undefined,null,0,true,[],{}, {status:'',state:'filled'},{status:false,state:'filled'},{status:0,state:'filled'}])streamRawCases.push({input:encode(input),result:capture(()=>stream.getRawOrderStatus(input))});
const conflictingStatusValues=[undefined,null,'',false,0,'FILLED','success',{toString:0}];
for(const status of conflictingStatusValues)for(const state of conflictingStatusValues)for(const planStatus of conflictingStatusValues)for(const orderStatus of conflictingStatusValues){
 const input={status,state,planStatus,orderStatus};
 streamRawCases.push({input:encode(input),result:capture(()=>stream.getRawOrderStatus(input))});
}
const streamTerminalCases=['triggered','filled','full_fill','success','executed','partial_fill','partially_filled','FULL_FILL','full-fill','','filled\uFEFF','\u0085filled','constructor'].map(input=>({input,value:stream.isTerminalFillStatus(input)}));
const streamFiniteCases=finiteCases.map(c=>{const input=c.input&&c.input.$number!==undefined?Number(c.input.$number):c.input;return {...c,value:encode(stream.toFiniteNumber(input,Number(c.fallback.$number)))};});
const streamExcluded=[NaN,Infinity,-Infinity].map(input=>({input:encode(input),json:JSON.stringify(input),status:stream.normalizeOrderStatus(input),channel:stream.normalizeChannel(input),raw:stream.getRawOrderStatus({status:input}),projected:stream.normalizeOrderStatus(JSON.parse(JSON.stringify(input)))}));
const utf16Stream=['\uD800-FILLED','A_\uDC00'].map(input=>({json:JSON.stringify(input),statusJson:JSON.stringify(stream.normalizeOrderStatus(input)),channelJson:JSON.stringify(stream.normalizeChannel(input)),rawJson:JSON.stringify(stream.getRawOrderStatus({status:input})),terminal:stream.isTerminalFillStatus(input)}));
const cancelPlanCases=[];
const rawPlans=[undefined,null,false,0,'','psl','ptp','pos_loss','pos_profit','loss_plan','profit_plan','track_plan','sl','tp','unknown','PSL','\uFEFFpsl',[],['psl'],['constructor'],{}, {toString:0},...Object.getOwnPropertyNames(Object.prototype)];
for(const raw of rawPlans)for(const type of Object.values(enums.OrderType))for(const amount of [0,-0,-1,0.4,1,2,NaN,Infinity])for(const step of [undefined,1]){
 const order={...strategyBase,type,amount,info:raw===undefined?{}:{planType:raw}};
 const errors=[];instance.emitAndLogError=message=>errors.push(message);
 const positions=[position('BTCUSDT','long',1),position('BTCUSDT','short',1)];
 instance.store={positions,marketsBySymbol:step===undefined?{}:{BTCUSDT:{precision:{amount:step}}}};
 let result;try{result={value:openValue(instance.getCancelPlanType(order))};}catch(error){result={error:{name:error.name,message:error.message}};}
 cancelPlanCases.push({order:encode(order),positions:encode(positions),step:step===undefined?undefined:encode(step),result,errors});
}
const utf16CancelPlan=['\uD800','x\uDC00'].map(symbol=>{const order={...strategyBase,symbol,type:enums.OrderType.Limit,info:{planType:'unknown'}};const errors=[];instance.emitAndLogError=message=>errors.push(message);instance.store={positions:[],marketsBySymbol:{}};let result;try{result={value:openValue(instance.getCancelPlanType(order))};}catch(error){result={error:{name:error.name,message:error.message}};}return {json:JSON.stringify(order),result,errorsJson:JSON.stringify(errors)};});
const payloadTimeCases=[];
const timeValues=[undefined,null,'',0,-0,-1,1,'17tail','\uFEFF18','\u008518','0x10','Infinity',true,[],{}, {toString:0}];
for(const key of ['uTime','updatedTime','utime','cTime','createdTime'])for(const value of timeValues){const info={uTime:null,updatedTime:null,utime:null,cTime:null,createdTime:7,[key]:value};payloadTimeCases.push({info:encode(info),result:openValue(stream.getPositionPayloadUpdateTime({info}))});}
for(const value of timeValues)for(const other of timeValues){const info={uTime:value,updatedTime:other,utime:'19',cTime:'20',createdTime:'21'};payloadTimeCases.push({info:encode(info),result:openValue(stream.getPositionPayloadUpdateTime({info}))});}
const payloadTimeExcluded=[NaN,Infinity,-Infinity].map(input=>{const position={info:{uTime:input,updatedTime:17}};return {input:encode(input),source:openValue(stream.getPositionPayloadUpdateTime(position)),projected:openValue(stream.getPositionPayloadUpdateTime(JSON.parse(JSON.stringify(position))))};});
const positionFoldCases=[];
function foldCase(rest,current,start,snapshotAt,snapshotIds,updated){
 stream.store={positions:current};stream.lastWsPositionSnapshotAt=snapshotAt;stream.lastWsPositionSnapshotIds=new Set(snapshotIds);stream.wsPositionUpdatedAtById=new Map(updated);stream.positionRestStaleGuardMs=5000;
 const result=stream.mergeRestPositionsWithFreshWsState(rest,start);
 const origins=result.map(p=>rest.includes(p)?['rest',rest.indexOf(p)]:['current',current.indexOf(p)]);
 positionFoldCases.push({rest:encode(rest),current:encode(current),start:encode(start),snapshotAt:encode(snapshotAt),snapshotIds,updated:encode(updated),origins});
}
const foldPosition=(id,time)=>({...position('BTCUSDT','long',1),...(fieldIdentity?{contracts:2.25,entryPrice:3.5,notional:4.75,leverage:5.25,unrealizedPnl:6.5,liquidationPrice:7.75}:{}),id,info:time===undefined?{}:{uTime:time}});
for(const restTime of [undefined,'',0,-0,-1,1,2,'junk','Infinity'])for(const currentTime of [undefined,0,-1,1,2,'junk'])for(const updated of [0,1,5000,5001,10000,15001,Infinity,NaN]){
 foldCase([foldPosition('p',restTime)],[foldPosition('p',currentTime)],10000,0,[],[['p',updated]]);
}
for(const start of [-Infinity,0,1,5000,5001,10000,Infinity,NaN])for(const snapshotAt of [0,1,5000,5001,15000,Infinity,NaN])for(const ids of [[],['p'],['q'],['p','q']]){
 foldCase([foldPosition('p',1),foldPosition('q',1)],[foldPosition('p',2),foldPosition('r',2)],start,snapshotAt,ids,[['p',5001],['r',5001]]);
}
const currentLayouts=[[],[foldPosition('p',1),foldPosition('p',2)],[foldPosition('p',2),foldPosition('p',1)],[foldPosition('r',3),foldPosition('p',2),foldPosition('p',1)]];
const restLayouts=[[],[foldPosition('p',0)],[foldPosition('p',0),foldPosition('p',0)],[foldPosition('q',0),foldPosition('p',0),foldPosition('q',0)]];
for(const rest of restLayouts)for(const current of currentLayouts)for(const snapshotIds of [[],['p']])foldCase(rest,current,10000,5001,snapshotIds,[['p',5001],['q',5001],['r',5001]]);
for(const id of ['constructor','__proto__','','\uFEFF','\u0085','A','a','𐐀','\u0000']){
 foldCase([foldPosition(id,1)],[foldPosition(id,2)],10000,5001,[id],[[id,5001]]);
 foldCase([foldPosition(id+'x',1)],[foldPosition(id,2)],10000,5001,[id+'x'],[[id,5001]]);
}
const utf16PositionFold=['\uD800','x\uDC00'].map(id=>{const rest=[foldPosition(id,1)],current=[foldPosition(id,2)];stream.store={positions:current};stream.lastWsPositionSnapshotAt=0;stream.lastWsPositionSnapshotIds=new Set();stream.wsPositionUpdatedAtById=new Map([[id,5001]]);return {json:JSON.stringify({rest,current}),selectedCurrent:stream.mergeRestPositionsWithFreshWsState(rest,10000)[0]===current[0]};});
const inverseOrder=JSON.parse(require('node:child_process').execFileSync(process.execPath,[path.join(__dirname,'inverse-order-capture.mjs'),...(fieldIdentity?['--field-identity']:[])],{encoding:'utf8',maxBuffer:16*1024*1024}));
if(fieldIdentity) mappedOrder('field-identity:all',{orderId:'identity-order',symbol:'IDENTITYUSDT',size:'13.5',price:'17.25',baseVolume:'2.75',presetStopLossPrice:'11.5',presetStopSurplusPrice:'23.75'});
const output=JSON.stringify({inverseOrder,payloadTimeExcluded,payloadTimeCases,positionFoldCases,utf16PositionFold,cancelPlanCases,utf16CancelPlan,streamExcluded,streamNormalizeCases,streamRawCases,streamTerminalCases,streamFiniteCases,utf16Stream,dynamicMapInventory,tableLookupInventory,mapOrderReference,mapOrderCases,mapOrderAlias,mapOrderUtf16,mapOrderNumericInfo,sideCases,triggers,ids,roundCases,sizingCases,utf16Ids,utf16Sizing,uuidCases,pnlCases,utf16Pnl,finiteCases,firstFiniteCases,utf16Finite,normalizeCases,utf16Normalize,successCases,liquidationCases,utf16Coercion,storePositionCases,profileCases,stateGetterCases,categoryCases,utf16Category,marketCases,utf16Market,symbolCases,utf16Symbol,utaPositionCases,classicPositionCases,utf16UtaPosition,strategyOptsCases,utf16StrategyOpts})+'\n';
if(process.argv.includes('--finding-policy')) {
 const members=['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'];
 const prototype=members.map(key=>{
  const record=instance.mapOrder({...baseOrder,status:key})[0];
  const expected=key==='__proto__'?Object.prototype:Object.prototype[key];
  assert.equal(record.status,expected,'native classic inherited status identity BEFORE serialization');
  return {key,kind:typeof record.status};
 });
 const utf16=['\ud800','x\udc00'].map(symbol=>{
  const value=instance.mapOrder({...baseOrder,symbol})[0].symbol;
  const units=Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
  assert.deepEqual(units,symbol==='\ud800'?[0xd800]:[0x78,0xdc00],'returned native symbol units BEFORE serialization');
  return {units,escaped:JSON.stringify(symbol)};
 });
 const input={...baseOrder,nested:{amount:NaN}};const record=instance.mapOrder(input)[0];
 assert.equal(record.info,input,'native info retains source alias');
 assert.ok(Number.isNaN(input.nested.amount));assert.ok(Number.isNaN(record.info.nested.amount),'native info accepts NaN before projection');
 assert.equal(prototype.length,12);assert.equal(utf16.length,2);
 process.stdout.write(JSON.stringify({prototype,utf16,nativeInfoNaN:true}));process.exit(0);
}
const fixture=path.join(__dirname,'../tests/order-oracle.json');
if(process.argv.includes('--write'))fs.writeFileSync(fixture,output);
else if(process.argv.includes('--check')){assert.equal(fs.readFileSync(fixture,'utf8'),output,'Bitget TS order oracle snapshot drift');console.log(`Bitget order oracle: ${sideCases.length} side, ${roundCases.length} rounding, ${sizingCases.length} sizing, ${pnlCases.length} PnL cases`);}
else process.stdout.write(output);
