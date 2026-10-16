const {preserveSpecialNumbers}=require('./special-numbers.cjs');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {indexedStringLiterals,indexedNumberLiterals}=require('./literal-tuples.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
const source=ts.createSourceFile('public.ts',fs.readFileSync(path.join(root,'coinbase.starbase-public.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const exportsObject={};
const protocol={};
const orderbook={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'../../utils/orderbook.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:orderbook});
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:protocol});
const updateBook={},sequencedBook={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'../../utils/updateOrderBook.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:updateBook});
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'../../utils/sequencedDeltaOrderBook.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:sequencedBook,require:name=>{assert.equal(name,'./updateOrderBook');return updateBook;}});
const functions=['parseBookState','parseBaseCurrency','parseStarbasePerpetualName','toCoinbaseStarbaseInstrumentName','toCoinbaseLegacyIntxSymbol','isRecord','protocolError','requireRecord','requireString','requireFinite','requirePositive','requirePositiveInteger','requireTimestamp'];
const declarations=['StarbaseBookAction','CoinbaseStarbaseOrderBookDepth','CoinbaseStarbaseBaseCurrency','CoinbaseStarbaseBookState','CoinbaseStarbaseBookInterval','CoinbaseStarbaseChartResolution','CoinbaseStarbaseMarketParityBlocker'];
const normalizers=['parseDeltaLevels','normalizeCoinbaseStarbaseTicker','normalizeCoinbaseStarbaseTickerPayload','parseTickSizeSteps','normalizeCoinbaseStarbaseInstrument','normalizeCoinbaseStarbaseInstruments','parseRestBookLevels','normalizeCoinbaseStarbaseOrderBookSnapshot','parseCandle','normalizeCoinbaseStarbaseChartPayload','normalizeCoinbaseStarbaseChartData'];
const names=['buildSubscriptionRequest','CoinbaseStarbasePublicSubscriptions','buildCoinbaseStarbaseTickerChannel','buildCoinbaseStarbaseBookChannel','buildCoinbaseStarbaseChartChannel','buildCoinbaseStarbaseInstrumentStateChannel','CoinbaseStarbaseSequencedOrderBook',...functions,...normalizers,'resolveChartResolution','COINBASE_STARBASE_TIMEFRAME_RESOLUTION','buildCoinbaseStarbaseChartData','assessCoinbaseStarbaseMarketParity','buildCoinbaseStarbaseOrderBook','buildCoinbaseStarbaseGetInstruments','buildCoinbaseStarbaseTicker','STARBASE_BOOK_DEPTHS','STARBASE_SUPPORTED_INSTRUMENT_NAMES','STARBASE_BASE_CURRENCIES','STARBASE_BOOK_STATES','STARBASE_BOOK_INTERVALS','COINBASE_STARBASE_CHART_RESOLUTIONS'];
const selected=source.statements.filter(n=>ts.isClassDeclaration(n)?names.includes(n.name.text):ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(source))));
assert.equal(selected.length,names.length,'all selected source definitions exist');
vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(source)).join('\n')+'\nexport {'+names.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject,...orderbook,...sequencedBook,CoinbaseStarbaseProtocolError:protocol.CoinbaseStarbaseProtocolError});
function literals(type){
 if(ts.isTypeReferenceNode(type))return literals(source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===type.typeName.getText(source)).type);
 if(ts.isUnionTypeNode(type))return type.types.flatMap(literals);
 if(ts.isLiteralTypeNode(type)&&ts.isStringLiteral(type.literal))return [type.literal.text];
 if(ts.isIndexedAccessTypeNode(type)){
  const {name,values}=indexedStringLiterals(ts,source,type);
  assert.deepEqual(Array.from(exportsObject[name]),values,'evaluated tuple must match declaration literals');
  return values;
 }

 throw Error('unsupported public literal declaration '+type.getText(source));
}
const schemas=Object.fromEntries(declarations.map(name=>[name,{kind:'enum',values:(()=>{const node=source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name).type;if(name!=='CoinbaseStarbaseOrderBookDepth')return literals(node);const result=indexedNumberLiterals(ts,source,node);assert.deepEqual(Array.from(exportsObject[result.name]),result.values);return result.values;})()}]));
const channelNode=source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='CoinbaseStarbasePublicChannel').type;
assert(ts.isUnionTypeNode(channelNode),'channel remains a template union');
const channelTemplates=channelNode.types.map(node=>{
 assert(ts.isTemplateLiteralTypeNode(node),'channel branch remains a template literal');
 return {head:node.head.text,spans:node.templateSpans.map(span=>({values:span.type.kind===ts.SyntaxKind.StringKeyword?null:literals(span.type),tail:span.literal.text}))};
});
schemas.CoinbaseStarbaseInstrumentStateCurrency={kind:'enum',values:channelTemplates.find(t=>t.head==='instrument.state.future.').spans[0].values};
const channelCases=[];
for(const template of channelTemplates){
 let entries=[{parts:[],text:template.head}];
 for(const span of template.spans)entries=entries.flatMap(entry=>(span.values??['','BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL','a.b','💰','\uFEFF','\u0085','\n','constructor','__proto__','a\u0000b']).map(value=>({parts:[...entry.parts,value],text:entry.text+value+span.tail})));
 for(const entry of entries)channelCases.push({head:template.head,...entry});
}
const subscriptionMethodNode=source.statements.filter(ts.isVariableStatement).flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='buildSubscriptionRequest').initializer.typeParameters[0].constraint;
schemas.CoinbaseStarbaseSubscriptionMethod={kind:'enum',values:literals(subscriptionMethodNode)};
const deltaDeclaration=source.statements.filter(ts.isVariableStatement).flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='parseDeltaLevels');
schemas.CoinbaseStarbaseBookFrameType={kind:'enum',values:literals(deltaDeclaration.initializer.parameters[1].type)};
function recordShape(node){
 if(node.kind===ts.SyntaxKind.UnknownKeyword)return {kind:'unknown'};
 if(node.kind===ts.SyntaxKind.StringKeyword)return {kind:'string'};
 if(node.kind===ts.SyntaxKind.NumberKeyword)return {kind:'number'};
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return {kind:'boolean'};
 if(ts.isTypeOperatorNode(node)&&node.operator===ts.SyntaxKind.ReadonlyKeyword)return recordShape(node.type);
 if(ts.isArrayTypeNode(node))return {kind:'array',item:recordShape(node.elementType)};
 if(ts.isTypeReferenceNode(node)){const name=node.typeName.getText(source);if(['Method','Params'].includes(name))return {kind:'parameter',name};if(name==='Record'){assert.equal(node.typeArguments.length,2);return {kind:'record',key:recordShape(node.typeArguments[0]),value:recordShape(node.typeArguments[1])};}return recordShape(source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name).type);}
 if(ts.isTypeLiteralNode(node))return {kind:'object',fields:node.members.map(field=>({name:field.name.text,optional:!!field.questionToken,schema:recordShape(field.type)}))};
 return {kind:'enum',values:literals(node)};
}
const recordSchemas=Object.fromEntries(['CoinbaseStarbaseMarketParityResult','CoinbaseStarbasePublicTicker','CoinbaseStarbasePublicInstrument','CoinbaseStarbasePublicRequest','ParsedStarbaseBookLevel'].map(name=>{const node=source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);return [name,{...recordShape(node.type),...(node.typeParameters?{parameters:node.typeParameters.map(p=>({name:p.name.text,constraint:recordShape(p.constraint),default:p.default?recordShape(p.default):null}))}:{})}];}));
for(const [name,field] of Object.entries({CoinbaseStarbaseUsdc:'quote',CoinbaseStarbaseFuture:'kind',CoinbaseStarbaseLinear:'instrumentType',CoinbaseStarbasePerpetual:'settlementPeriod'}))schemas[name]=recordSchemas.CoinbaseStarbasePublicInstrument.fields.find(f=>f.name===field).schema;
schemas.CoinbaseStarbaseMarketParityStatus=recordSchemas.CoinbaseStarbaseMarketParityResult.fields.find(f=>f.name==='status').schema;
const values=[undefined,null,false,true,0,-0,-1,1,0.1,1e-30,1e21,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,'',' ','\uFEFF','\u0085','1','0x10','x',[],{},['x'],{a:1}];
const contexts=['field','','\uFEFF','\u0085','💰','line\nbreak'];
function capture(fn){try{return {ok:fn()}}catch(e){return {error:{name:e.name,message:e.message}}}}
const namesInputs=[...values,...exportsObject.STARBASE_BOOK_STATES,...exportsObject.STARBASE_BASE_CURRENCIES];
for(const base of ['BTC','ETH','SOL','USDC','1','BTC1','btc','', 'ÉTH', 'ＢＴＣ'])for(const suffix of ['_USDC-PERPETUAL','-PERP-INTX','_USD-PERPETUAL']){
 const name=base+suffix;namesInputs.push(name);
 for(const char of [' ', '\t','\n','\r','\uFEFF','\u0085','\u2028','\u2029','_','/'])namesInputs.push(char+name,name+char);
}
for(const name of [...exportsObject.STARBASE_BOOK_STATES,...exportsObject.STARBASE_BASE_CURRENCIES])namesInputs.push(name.toLowerCase(),name.toUpperCase(),{[name]:null},[name],' '+name,name+'\uFEFF');
const nameCases=[];
for(const name of ['parseBookState','parseBaseCurrency','parseStarbasePerpetualName','toCoinbaseStarbaseInstrumentName','toCoinbaseLegacyIntxSymbol'])for(const value of namesInputs){
 if(name.startsWith('toCoinbase')&&typeof value!=='string')continue;
 nameCases.push({name,...value===undefined?{}:{input:value},context:'field',result:capture(()=>exportsObject[name](value,'field'))});
}
const cases=[];
for(const name of functions.filter(n=>!['requireFinite','protocolError','parseBookState','parseBaseCurrency','parseStarbasePerpetualName','toCoinbaseStarbaseInstrumentName','toCoinbaseLegacyIntxSymbol'].includes(n)))for(const value of values)for(const context of contexts)
 cases.push({name,...value===undefined?{}:{input:value},context,result:capture(()=>exportsObject[name](value,context))});
const minima=[undefined,0,-0,-10,0.1,1e-7,1e21,NaN,Infinity,-Infinity];
const finite=[];
for(const value of values)for(const minimum of minima)for(const context of contexts){
 const spec=minimum===undefined?'default':Number.isNaN(minimum)?'NaN':minimum===Infinity?'Infinity':minimum===-Infinity?'-Infinity':minimum;
 finite.push({...value===undefined?{}:{input:value},minimum:spec,context,result:capture(()=>exportsObject.requireFinite(value,context,minimum))});
}
const tickerBase=JSON.parse(fs.readFileSync(path.join(root,'../../__tests__/audit-regressions/coinbase.starbase-live-public.evidence/ticker.json'),'utf8')).result;
const instrumentEvidence=JSON.parse(fs.readFileSync(path.join(root,'../../__tests__/audit-regressions/coinbase.starbase-live-public.evidence/instruments.json'),'utf8'));
const tickerInputs=[...values,tickerBase,{...tickerBase,instrument_name:'ETH_USDC-PERPETUAL'},Object.values(tickerBase)];
const paths=[...Object.keys(tickerBase).filter(k=>k!=='stats'),...Object.keys(tickerBase.stats).map(k=>'stats.'+k)];
function changed(changes){
 const value=structuredClone(tickerBase);
 for(const [key,v]of changes){const parts=key.split('.');const target=parts.length===2?value[parts[0]]:value;const field=parts.at(-1);if(v===undefined)delete target[field];else target[field]=v;}
 return value;
}
for(const key of [...paths,'stats'])for(const value of [...values,'open','BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL',-0.05,9007199254740990])tickerInputs.push(changed([[key,value]]));
for(const left of paths)for(const right of paths)if(left!==right)tickerInputs.push(changed([[left,{}],[right,{}]]));
const optionalPaths=['stats.price_change','stats.volume_usd','current_funding','funding_8h'];
for(let mask=0;mask<256;mask++)tickerInputs.push(changed(optionalPaths.map((key,i)=>[key,[undefined,null,0,-0.005][(mask>>(i*2))&3]])));
const canonical=value=>value===undefined?undefined:JSON.parse(JSON.stringify(value));
const tickerCases=tickerInputs.map(canonical).map(input=>({...input===undefined?{}:{input},result:capture(()=>exportsObject.normalizeCoinbaseStarbaseTicker(input))}));
const tickerIdentitySupplementInput=changed([['timestamp',1234],['best_bid_price',10.01],['best_bid_amount',0.11],['best_ask_price',10.02],['best_ask_amount',0.22],['last_price',10.03],['mark_price',10.04],['index_price',10.05],['stats.price_change',0.06],['current_funding',0.0007],['funding_8h',0.0008],['stats.volume',0.9],['stats.volume_usd',1.01],['open_interest',2.02]]);
const tickerIdentitySupplement={label:'field-identity-supplement',input:tickerIdentitySupplementInput,result:capture(()=>exportsObject.normalizeCoinbaseStarbaseTicker(tickerIdentitySupplementInput))};
const payloadInputs=[...values,...tickerInputs.map(data=>({data})),{data:tickerBase,other:null}];
const tickerPayloadCases=payloadInputs.map(canonical).map(input=>({...input===undefined?{}:{input},result:capture(()=>exportsObject.normalizeCoinbaseStarbaseTickerPayload(input))}));

const fixtures={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'../../__tests__/audit-regressions/coinbase.starbase-public.fixtures.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:fixtures});
const btc=fixtures.STARBASE_PUBLIC_INSTRUMENT_FIXTURE,eth=fixtures.STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE;
function instrumentChanged(changes){const value=structuredClone(btc);for(const [key,v]of changes){if(v===undefined)delete value[key];else value[key]=v;}return value;}
const instrumentInputs=[...values,btc,eth,Object.values(btc)];
const instrumentFields=[...Object.keys(btc),'option_type'];
const instrumentValues=[...values,'future','linear','crypto','perpetual','USDC','BTC','ETH','open','BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL',9007199254740990];
for(const key of instrumentFields)for(const value of instrumentValues)instrumentInputs.push(instrumentChanged([[key,value]]));
for(const left of instrumentFields)for(const right of instrumentFields)if(left!==right)instrumentInputs.push(instrumentChanged([[left,{}],[right,{}]]));
for(let mask=0;mask<256;mask++)instrumentInputs.push(instrumentChanged(['future_type','state','max_leverage','tick_size_steps'].map((key,i)=>[key,[undefined,null,btc[key],{}][(mask>>(2*i))&3]])));
const stepInputs=[...values,[],[{above_price:1,tick_size:0.01}],[{above_price:2,tick_size:0.1},{above_price:1,tick_size:0.01}]];
for(const value of values){stepInputs.push([value],[{above_price:value,tick_size:1}],[{above_price:1,tick_size:value}],[{above_price:0,tick_size:value}]);}
for(const steps of stepInputs)instrumentInputs.push(instrumentChanged([['tick_size_steps',steps]]));
const instrumentListInputs=[...values,fixtures.STARBASE_PUBLIC_INSTRUMENT_DISCOVERY_FIXTURE,[btc,eth],[eth,btc],[btc],[eth],[btc,btc],[btc,btc,eth],[eth,btc,eth]];
for(const input of instrumentInputs)instrumentListInputs.push([input,eth],[btc,input],[btc,btc,input]);
for(const input of values)instrumentListInputs.push([instrumentChanged([['kind',null]]),input],[{instrument_name:'SOL_USDC-PERPETUAL',kind:null},btc,eth,input]);
function corpus(inputs,name){return inputs.map(canonical).map(input=>({...input===undefined?{}:{input},result:capture(()=>exportsObject[name](input))}));}
const instrumentCases=corpus(instrumentInputs,'normalizeCoinbaseStarbaseInstrument');
const instrumentIdentitySupplementInput=instrumentChanged([['min_trade_amount',0.0002],['contract_size',0.0003],['tick_size',0.0004],['creation_timestamp',1234],['expiration_timestamp',5678]]);
const instrumentIdentitySupplement={label:'field-identity-supplement',input:instrumentIdentitySupplementInput,result:capture(()=>exportsObject.normalizeCoinbaseStarbaseInstrument(instrumentIdentitySupplementInput))};
const ethInstrumentIdentitySupplementInput={...eth,min_trade_amount:0.0012,contract_size:0.0023,tick_size:0.0034,creation_timestamp:2345,expiration_timestamp:6789};
const ethInstrumentIdentitySupplement={label:'field-identity-supplement',input:ethInstrumentIdentitySupplementInput,result:capture(()=>exportsObject.normalizeCoinbaseStarbaseInstrument(ethInstrumentIdentitySupplementInput))};
const evidenceInstrument=instrumentEvidence.results.find(item=>item.instrument_name==='BTC_USDC-PERPETUAL');
const evidenceInstrumentIdentitySupplementInput={...evidenceInstrument,min_trade_amount:0.0005,contract_size:0.0006,tick_size:0.0007,creation_timestamp:3456,expiration_timestamp:7890};
const evidenceInstrumentIdentitySupplement={label:'field-identity-supplement: live REST instruments.json',input:evidenceInstrumentIdentitySupplementInput,result:capture(()=>exportsObject.normalizeCoinbaseStarbaseInstrument(evidenceInstrumentIdentitySupplementInput))};
const instrumentListCases=corpus(instrumentListInputs,'normalizeCoinbaseStarbaseInstruments');
const stepCases=corpus(stepInputs,'parseTickSizeSteps');


const restLevels=[...values,[],[[2,0.1],[1,0.2],[2,0.3]],[[1,1e308],[2,1e308]],[[1,0]],[[1,2],[2,3],[3,4]]];
for(const value of values)restLevels.push([value],[[value,1]],[[1,value]],[[value,{}]],[[1,1],value]);
restLevels.push([[1]],[[1,2,3]],[[1,2],[]]);
const bookInputs=[...values,fixtures.STARBASE_PUBLIC_ORDER_BOOK_FIXTURE];
for(const bids of restLevels)for(const asks of [[],[[1,2]],null])bookInputs.push({bids,asks});
for(const asks of restLevels)bookInputs.push({bids:[[1,2]],asks});
const restLevelCases=[];
for(const input of restLevels.map(canonical))for(const context of contexts)restLevelCases.push({...input===undefined?{}:{input},context,result:capture(()=>exportsObject.parseRestBookLevels(input,context))});
const bookCases=bookInputs.map(canonical).map(input=>({...input===undefined?{}:{input},result:preserveSpecialNumbers(capture(()=>exportsObject.normalizeCoinbaseStarbaseOrderBookSnapshot(input)))}));
// Raw JSON preserves -0, which fixture JSON.stringify would turn into +0.
for(const inputJson of ['{"bids":[[1,-0]],"asks":[[2,-0]]}','{"bids":[[3,0],[2,-0],[1,1]],"asks":[[2,-0],[3,0]]}'])bookCases.push({inputJson,result:preserveSpecialNumbers(capture(()=>exportsObject.normalizeCoinbaseStarbaseOrderBookSnapshot(JSON.parse(inputJson))))});
const candleBase={tick:1784139975074,open:2,high:3,low:1,close:2.5,volume:0.25};
const candleInputs=[candleBase,{},Object.fromEntries(Object.keys(candleBase).map(k=>[k,1]))];
for(const field of Object.keys(candleBase))for(const value of values)candleInputs.push({...candleBase,[field]:value});
for(const left of Object.keys(candleBase))for(const right of Object.keys(candleBase))candleInputs.push({...candleBase,[left]:{},[right]:null});
const candleCases=[];
for(const input of candleInputs.map(canonical))for(const context of contexts)candleCases.push({input,context,result:capture(()=>exportsObject.parseCandle(input,context))});
const chartPayloadCases=corpus([...values,...values.map(data=>({data})),...candleInputs.map(data=>({data}))],'normalizeCoinbaseStarbaseChartPayload');
const chartBase={status:'ok',ticks:[3000,1000,1000],open:[2,3,4],high:[3,4,5],low:[1,2,3],close:[2,3,4],volume:[0,0.1,0.2]};
const chartInputs=[...values,chartBase,{status:'ok',ticks:[],open:[],high:[],low:[],close:[],volume:[]},{status:'no_data'}, {status:'no_data',ticks:null,open:42}];
for(const field of Object.keys(chartBase))for(const value of [...values,[],[1],[1,2],[1,2,3,4]])chartInputs.push({...chartBase,[field]:value});
for(const field of Object.keys(chartBase).filter(k=>k!=='status'))for(const value of values)chartInputs.push({...chartBase,[field]:[1,value,3]});
for(const left of Object.keys(chartBase))for(const right of Object.keys(chartBase))chartInputs.push({...chartBase,[left]:null,[right]:{}});
const chartCases=corpus(chartInputs,'normalizeCoinbaseStarbaseChartData');

// Actual source builders are type-serialization witnesses, not new ported builders.
const requestSamples=[exportsObject.buildCoinbaseStarbaseGetInstruments(),exportsObject.buildCoinbaseStarbaseTicker('ETH-PERP-INTX')];

const deltaInputs=[...values,[],[['new',1,1]],[['change',2,1]],[['delete',1,0]],[['new',3,1],['delete',2,0],['change',1,2]]];
for(const value of values){deltaInputs.push([value],[['new',1,1],value]);for(const action of ['new','change','delete'])deltaInputs.push([[action,value,1]],[[action,1,value]],[[action,{},value]]);deltaInputs.push([[value,1,1]],[[value,{},{}]]);}
for(const entry of [['new'],['new',1],['new',1,1,1],{action:'new',amount:1,price:1},['NEW',1,1],['new\uFEFF',1,1]])deltaInputs.push([entry]);
const deltaCases=[];
for(const input of deltaInputs.map(canonical))for(const frameType of schemas.CoinbaseStarbaseBookFrameType.values)for(const context of contexts)deltaCases.push({...input===undefined?{}:{input},frameType,context,result:preserveSpecialNumbers(capture(()=>exportsObject.parseDeltaLevels(input,frameType,context)))});

for(const inputJson of ['[["delete",1,-0]]','[["new",1,-0]]','[["change",1,-0]]'])for(const frameType of schemas.CoinbaseStarbaseBookFrameType.values)deltaCases.push({inputJson,frameType,context:'field',result:preserveSpecialNumbers(capture(()=>exportsObject.parseDeltaLevels(JSON.parse(inputJson),frameType,'field')))});
const requestCases=[{name:'instruments',result:capture(()=>exportsObject.buildCoinbaseStarbaseGetInstruments())}];
for(const symbol of namesInputs.filter(value=>typeof value==='string')){
 requestCases.push({name:'ticker',symbol,result:capture(()=>exportsObject.buildCoinbaseStarbaseTicker(symbol))});
 for(const depth of [...values,...exportsObject.STARBASE_BOOK_DEPTHS,1.1,999,10001])requestCases.push({name:'book',symbol,...depth===undefined?{}:{depth},result:capture(()=>exportsObject.buildCoinbaseStarbaseOrderBook(symbol,depth))});
}
const marketParityCases=instrumentCases.filter(c=>c.result.ok).map(c=>{
 const gateway=exportsObject.normalizeCoinbaseStarbaseInstrument(c.input);
 const result=exportsObject.assessCoinbaseStarbaseMarketParity(gateway);
 assert.equal(result.gateway,gateway,'assess must retain gateway identity');
 return {input:c.input,result};
});
const prototypeKeys=Object.getOwnPropertyNames(Object.prototype);
const timeframes=[...Object.keys(exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION),'4h','1w','', '1M',' 1m','1m\uFEFF','\u00851m',...prototypeKeys];
const channelBuilderCases=[];
for(const symbol of namesInputs.filter(v=>typeof v==='string')){
 for(const interval of [...values,...exportsObject.STARBASE_BOOK_INTERVALS,'AGG2','100MS','agg2\uFEFF']){
  for(const kind of ['Ticker','Book'])channelBuilderCases.push({kind,symbol,...interval===undefined?{}:{interval},result:capture(()=>exportsObject['buildCoinbaseStarbase'+kind+'Channel'](symbol,interval))});
 }
 for(const timeframe of timeframes){
  const result=capture(()=>exportsObject.buildCoinbaseStarbaseChartChannel(symbol,timeframe));
  const prototype=prototypeKeys.includes(timeframe);
  if(prototype&&result.ok){
   const inherited=Reflect.get(Object.getPrototypeOf(exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION),timeframe,exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION);
   assert.equal(result.ok,`chart.trades.${exportsObject.toCoinbaseStarbaseInstrumentName(symbol)}.${inherited}`,'inherited channel coercion');
  }
  channelBuilderCases.push({kind:'Chart',symbol,timeframe,prototype,result});
 }
}
for(const currency of [...values,...exportsObject.STARBASE_BASE_CURRENCIES,'USDC','any','btc','Any','BTC\uFEFF',...prototypeKeys])channelBuilderCases.push({kind:'InstrumentState',...currency===undefined?{}:{currency},result:capture(()=>exportsObject.buildCoinbaseStarbaseInstrumentStateChannel(currency))});
const resolutionCases=timeframes.map(timeframe=>{
 const result=capture(()=>exportsObject.resolveChartResolution(timeframe));
 if(prototypeKeys.includes(timeframe))assert.equal(result.ok,Reflect.get(Object.getPrototypeOf(exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION),timeframe,exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION),'inherited resolution identity');
 return {timeframe,prototype:prototypeKeys.includes(timeframe),result,actualType:typeof result.ok};
});
const chartRequestCases=[];
function chartRequest(symbol,timeframe,start,end){
 const result=capture(()=>exportsObject.buildCoinbaseStarbaseChartData(symbol,timeframe,start,end));
 if(prototypeKeys.includes(timeframe)&&result.ok)assert.equal(result.ok.params.resolution,Reflect.get(Object.getPrototypeOf(exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION),timeframe,exportsObject.COINBASE_STARBASE_TIMEFRAME_RESOLUTION),'inherited request resolution identity');
 chartRequestCases.push({symbol,timeframe,...start===undefined?{}:{start},...end===undefined?{}:{end},prototype:prototypeKeys.includes(timeframe),result:preserveSpecialNumbers(result),resolutionType:typeof result.ok?.params.resolution});
}
for(const timeframe of timeframes)for(const symbol of ['BTC-PERP-INTX','ETH_USDC-PERPETUAL','bad'])for(const [start,end] of [[0,1],[1,0],[1,1],[null,0],[0,null],[null,null],[9007199254740990,9007199254740991]])chartRequest(symbol,timeframe,start,end);
for(const start of values.map(canonical))for(const end of values.map(canonical))chartRequest('BTC-PERP-INTX','1m',start,end);
// Preserve raw signed zero input; normal corpus serialization canonicalizes -0.
for(const start of [0,-0])for(const end of [0,-0]){
 const startJson=Object.is(start,-0)?'-0':'0',endJson=Object.is(end,-0)?'-0':'0';
 const result=exportsObject.buildCoinbaseStarbaseChartData('BTC-PERP-INTX','1m',start,end);
 chartRequestCases.push({symbol:'BTC-PERP-INTX',timeframe:'1m',startJson,endJson,prototype:false,result:preserveSpecialNumbers({ok:result})});
}
function frame(type,id=10,prev=9,bids,asks){return {data:{type,instrument_name:'BTC_USDC-PERPETUAL',change_id:id,prev_change_id:prev,bids:bids??[[type==='snapshot'?'new':'change',10,2]],asks:asks??[[type==='snapshot'?'new':'change',11,3]]}};}
const snap=frame('snapshot'),change=frame('change',11,10),gap=frame('change',12,5),invalidAction=frame('change',11,10,[['new',10,1]]);
const reset={reconnect:true};
const preambles=[[],[snap],[change],[snap,gap],[snap,invalidAction],[snap,reset]];
const frameInputs=[...values.map(canonical),snap,change,gap,invalidAction,frame('snapshot',11,0,[['new',10,1],['new',10,2]]),frame('snapshot',11,0,[['new',10,Number.MAX_VALUE],['new',9,Number.MAX_VALUE]],[])];
for(const kind of ['snapshot','change']){
 for(const field of ['type','instrument_name','change_id','prev_change_id','bids','asks'])for(const value of values.map(canonical))frameInputs.push({data:{...frame(kind,11,10).data,[field]:value}});
 for(const id of [0,9,10,11,12,9007199254740991,9007199254740992])for(const prev of [9,10,11])frameInputs.push(frame(kind,id,prev));
 for(const side of ['bids','asks'])for(const levels of [[],[['new',12,1]],[['change',12,1]],[['delete',10,0]],[['delete',12,0]],[['change',10,1],['change',10,2]],[['delete',10,0],['new',10,1]],[['new',12,1],['delete',12,0]],[['new',12,1],['new',12,2]], [['change',10,1],['delete',12,0]]])frameInputs.push({data:{...frame(kind,11,10).data,[side]:levels}});
}
function sequenceCase(events){
 const instance=new exportsObject.CoinbaseStarbaseSequencedOrderBook(), identity=instance.orderBook;
 return events.map(event=>{
  const wrapped=event.reconnect||event.externalBook?event:Object.hasOwn(event,'input')?event:{input:event};
  let result;
  if(wrapped.reconnect){instance.onReconnect();result={ok:null};}
  else if(wrapped.externalBook){Object.assign(identity,structuredClone(wrapped.externalBook));result={ok:null};}
  else result=capture(()=>instance.applyPayload(wrapped.input));
  assert.equal(instance.orderBook,identity,'persistent Coinbase orderBook identity');
  return {...wrapped,result:preserveSpecialNumbers(result),book:preserveSpecialNumbers(instance.orderBook)};
 });
}
const sequencedCases=[];
for(const before of preambles)for(const input of frameInputs)sequencedCases.push(sequenceCase([...before,{input},{input:frame('change',12,11)},{input:frame('snapshot',20,0)},{input:frame('change',21,20)},reset]));
for(const price of [9,10,12])sequencedCases.push(sequenceCase([
 snap,{externalBook:{bids:[{price,amount:5,total:99}],asks:[]}},
 frame('change',11,10,[['change',price,2]],[]),
 frame('change',12,11,[['delete',price,0]],[]),reset,
]));
// Replay of a previously valid new level is checked against the mutated book
// before shared duplicate/stale sequence filtering. Preserve this TS defect.
const addLevel=frame('change',11,10,[['new',12,1]],[]);
sequencedCases.push(sequenceCase([snap,addLevel,addLevel,frame('snapshot',20,0)]));
sequencedCases.push(sequenceCase([snap,frame('change',9,8,[['new',10,1]],[]),frame('snapshot',20,0)]));
function subscriptionCapture(fn){try{const value=fn();return value===undefined?{undefined:true}:{ok:value};}catch(error){return {error:{name:error.name,message:error.message}};}}
const subscriptionCases=[];
for(let i=0;i<channelCases.length;i++){
 const a=i,b=(i+1)%channelCases.length,c=(i+11)%channelCases.length;
 for(const initial of [[],[a],[b,a],[a,b,c]]){
  const instance=new exportsObject.CoinbaseStarbasePublicSubscriptions();
  const operations=[{op:'reconnect'},...initial.length?[{op:'subscribe',channels:initial}]:[],
   {op:'subscribe',channels:[]},{op:'subscribe',channels:[a,a]},
   {op:'subscribe',channels:[a,b]},{op:'subscribe',channels:[b,a]},
   {op:'unsubscribe',channels:[]},{op:'unsubscribe',channels:[b,b]},
   {op:'unsubscribe',channels:[a]},{op:'subscribe',channels:[a]},
   {op:'unsubscribe',channels:[a,b,c]},{op:'unsubscribe',channels:[a]},
  ];
  const steps=operations.map(operation=>{
   const channels=operation.channels?.map(index=>channelCases[index].text);
   const result=subscriptionCapture(()=>operation.op==='reconnect'?instance.buildReconnectRequest():instance[operation.op](channels));
   const expected=structuredClone(result);
   // Both the input array and returned request are independent from retained state.
   channels?.splice(0,channels.length,'mutated-input');
   assert.equal(JSON.stringify(result),JSON.stringify(expected),'request must copy caller channels');
   if(result.ok)result.ok.params.channels.splice(0,result.ok.params.channels.length,'mutated-output');
   return {...operation,result:expected,reconnect:subscriptionCapture(()=>instance.buildReconnectRequest())};
  });
  subscriptionCases.push(steps);
 }
}
const acknowledgementCases=[];
for(let i=0;i<channelCases.length;i++){
 const a=i,b=(i+1)%channelCases.length;
 for(const requested of [[],[a],[a,b],[a,a]]){
  const names=requested.map(index=>channelCases[index].text);
  for(const value of [...values,[],names,[...names].reverse(),[channelCases[a].text],[channelCases[b].text],[channelCases[a].text,channelCases[b].text],[...names,'extra']]){
   const instance=new exportsObject.CoinbaseStarbasePublicSubscriptions();
   acknowledgementCases.push({requested,...value===undefined?{}:{value},result:subscriptionCapture(()=>instance.validateAcknowledgement(value,names))});
  }
 }
}
process.stdout.write(JSON.stringify({subscriptionCases,acknowledgementCases,channelBuilderCases,channelTemplates,channelCases,sequencedCases,resolutionCases,chartRequestCases,marketParityCases,requestCases,deltaCases,requestSamples,restLevelCases,bookCases,candleCases,chartPayloadCases,chartCases,instrumentCases,instrumentIdentitySupplement,ethInstrumentIdentitySupplement,evidenceInstrumentIdentitySupplement,instrumentListCases,stepCases,schemas,recordSchemas,tickerCases,tickerIdentitySupplement,tickerPayloadCases,cases,finite,nameCases,errors:contexts.map(context=>({context,result:capture(()=>exportsObject.protocolError(context))}))}));
