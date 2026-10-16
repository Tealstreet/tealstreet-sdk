const fs=require('node:fs'),path=require('node:path');
const {ts,root}=require('./source.cjs');
const printer=ts.createPrinter();
const file=path.join(root,'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='AsterDEXExchange');
const names=['markPositionStateUpdatedByWebsocket','getPositionStateVersion','markBalanceStateUpdatedByWebsocket','getBalanceStateVersion','assertV3Feature','transformOrder','getAccountPositionMode','transformMarket','extractFilterValue','reverseMapOrderType','reverseMapOrderStatus','canSign','getFapiPath','getAccountEndpoint','mapOrderType','mapTimeframe','getTickSize','assertAlgoOrderCapacity','getMaxAlgoOrdersLimit','getOpenAlgoOrdersCount','transformTicker','transformPosition','getOrderPositionSide','collectOpenInterestFromPayload','fetchOpenInterestMap','isAlgoOrder','reverseMapTimeInForce'];
const members=names.map(name=>{const node=cls.members.find(n=>n.name?.getText(ast)===name);if(!node)throw Error('Missing '+name);let printed=printer.printNode(ts.EmitHint.Unspecified,node,ast);if(name==='fetchOpenInterestMap'&&process.env.ASTERDEX_OPEN_INTEREST_CONTROL==='always-fallback'){const original='if (map.size > 0 || !allowPerSymbolFallback || !symbols?.length)';if(!printed.includes(original))throw Error('Open-interest source control target missing');printed=printed.replace(original,'if (!allowPerSymbolFallback || !symbols?.length)');}if(name==='reverseMapOrderStatus'&&process.env.ASTERDEX_STATUS_CONTROL==='swapped'){const original='return mapping[status] || OrderStatus.Open;';if(!printed.includes(original))throw Error('Status control source missing');printed=printed.replace(original,'return status === "constructor" ? Object.prototype.toString : mapping[status] || OrderStatus.Open;');}if(name==='extractFilterValue'&&process.env.ASTERDEX_FILTER_CONTROL==='own-only'){const original='filter && filter[field]';if(!printed.includes(original))throw Error('Filter control source missing');printed=printed.replace(original,'filter && Object.hasOwn(filter, field) && filter[field]');}if(name==='transformMarket'&&process.env.ASTERDEX_MARKET_CONTROL==='nullish'){const original='leverageBrackets.get(market.symbol) || 10';if(!printed.includes(original))throw Error('Market control source missing');printed=printed.replace(original,'leverageBrackets.get(market.symbol) ?? 10');}if(name==='transformMarket'&&process.env.ASTERDEX_MARKET_CONTROL==='nonfinite-precision'){const original='parseFloat(precisionToString(pricePrecision))';if(!printed.includes(original))throw Error('Nonfinite precision control source missing');printed=printed.replace(original,'Number.isFinite(pricePrecision) ? parseFloat(precisionToString(pricePrecision)) : 0');}if(name==='getAccountPositionMode'&&process.env.ASTERDEX_MODE_CONTROL==='inverted'){const original='this.store.options.isHedged ?';if(!printed.includes(original))throw Error('Account mode control source missing');printed=printed.replace(original,'!this.store.options.isHedged ?');}if(name==='transformOrder'&&process.env.ASTERDEX_ORDER_CONTROL==='zero-stop'){const original='parseFloat(order.stopPrice) > 0';if(!printed.includes(original))throw Error('Order control source missing');printed=printed.replace(original,'parseFloat(order.stopPrice) >= 0');}if(name==='assertV3Feature'&&process.env.ASTERDEX_FEATURE_CONTROL==='return-error'){const original='throw new Error(message);';if(!printed.includes(original))throw Error('Feature guard control source missing');printed=printed.replace(original,'return new Error(message);');}if(name==='markBalanceStateUpdatedByWebsocket'&&process.env.ASTERDEX_VERSION_CONTROL==='postincrement'){const original='this.balanceStateVersion += 1;\n    return this.balanceStateVersion;';if(!printed.includes(original))throw Error('Balance version control source missing');printed=printed.replace(original,'return this.balanceStateVersion++;');}return printed}).join('\n');
const unknown=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='isAsterDEXUnknownOrderError'));
const shared=ts.createSourceFile('types.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const enums=shared.statements.filter(ts.isEnumDeclaration).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,shared)).join('\n');
const idAst=ts.createSourceFile('id.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const idSource=idAst.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,idAst)).join('\n');
const typeAst=ts.createSourceFile('asterdex.types.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts'),'utf8'),ts.ScriptTarget.Latest,true);const orderMap=typeAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(typeAst)==='asterdexOrderTypeMap'));
const precisionSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/ccxt_helpers/precision.ts'),'utf8');
const precisionExports={};new Function('exports',ts.transpileModule(precisionSource,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(precisionExports);
const source='const LogSeverity={Warning:"Warning"};\n'+enums+'\n'+printer.printNode(ts.EmitHint.Unspecified,orderMap,typeAst)+'\n'+idSource+'\n'+printer.printNode(ts.EmitHint.Unspecified,unknown,ast)+'\nclass Oracle {store:any;\n'+members+'\n}\nexport {Oracle,isAsterDEXUnknownOrderError};';
const out={exports:{}};new Function('exports','hasAsterDEXV3AuthContext','getAsterDEXFapiPath','precisionToString',ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(out.exports,require('./source.cjs').values.hasAsterDEXV3AuthContext,require('./source.cjs').values.getAsterDEXFapiPath,precisionExports.precisionToString);
const {Oracle,isAsterDEXUnknownOrderError}=out.exports,o=new Oracle();
const result=fn=>{try{return {value:fn()}}catch(e){return {error:{name:e.name,message:e.message}}}};
const side=[];for(const isHedged of [true,false])for(const type of ['limit','market','conditional','stop_market','take_profit_market','trailing_stop_market'])for(const orderSide of ['buy','sell'])for(const reduceOnly of [undefined,false,true]){const opts={symbol:'BTC',side:orderSide,amount:1,type,...(reduceOnly===undefined?{}:{reduceOnly})};o.store={options:{isHedged}};side.push({opts,isHedged,output:o.getOrderPositionSide(opts)});}
const inputs=[undefined,null,false,[],{}, {symbol:'BTC',openInterest:null},{symbol:'BTC',openInterest:'0x10'}, {data:{symbol:'ETH',openInterest:[]}},[{symbol:'BTC',openInterest:'2'},null,{symbol:'BTC',openInterest:'3'},{symbol:'ETH',openInterest:'12abc'}],{data:[{symbol:'BTC',openInterest:true},{symbol:'__proto__',openInterest:7}]},[{symbol:'2',openInterest:1},{symbol:'1',openInterest:2},{symbol:'2',openInterest:3}],[{symbol:'BTC',openInterest:1},{symbol:'',openInterest:{toString:null}},{symbol:'ETH',openInterest:2}],{data:null,symbol:'ROOT',openInterest:'1e309'},{data:[],symbol:'ROOT',openInterest:5}];
const interest=inputs.map(input=>{const map=new Map([['existing',3]]),outcome=result(()=>{o.collectOpenInterestFromPayload(input,map);return null});return {input,...outcome,entries:[...map]}});
const algo=[];for(const type of ['limit','market','conditional','stop_market','take_profit_market','trailing_stop_market'])for(const info of [undefined,null,{}, {type:'STOP'},{type:'STOP_MARKET'},{type:'TRAILING_STOP'},{type:'TRAILING_STOP_MARKET'},{type:'TAKE_PROFIT'},{type:'TAKE_PROFIT_MARKET'},{type:'stop'},{type:7}])algo.push({type,info,output:o.isAlgoOrder({type,info})});
const unknownErrors=[undefined,null,false,'code: -2011 unknown order','code: -2011 UNKNOWN ORDER','code:-2011 unknown order',{message:'code: -2011 unknown order'},{toString:null},new Error('code: -2011 Unknown Order')].map(error=>({input:error,...(error instanceof Error?{nativeMessage:error.message}:{}),output:result(()=>isAsterDEXUnknownOrderError(error))}));
const tif=['','GTC','PostOnly',' ','\ufeff','constructor'].map(input=>({input,output:o.reverseMapTimeInForce(input)}));
const clockFile=path.join(root,'packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts');
const clockAst=ts.createSourceFile(clockFile,fs.readFileSync(clockFile,'utf8'),ts.ScriptTarget.Latest,true);
const clockClass=clockAst.statements.find(n=>ts.isClassDeclaration(n));
const clockMembers=['getTimePathCandidates','parseServerTime'].map(name=>{const n=clockClass.members.find(n=>n.name?.getText(clockAst)===name);if(!n)throw Error('Missing '+name);return printer.printNode(ts.EmitHint.Unspecified,n,clockAst)}).join('\n');
const clockExports={};new Function('exports',ts.transpileModule('export class Clock {'+clockMembers+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(clockExports);const clock=new clockExports.Clock();
const serverTimes=[undefined,null,true,false,0,-1,1,0.5,8640000000000001,'',' ','\ufeff','\u0085','0x10','12abc','1e309','Infinity','0','-2','0.1',[],[1],{}, {toString:null}].map(input=>({input,output:clock.parseServerTime(input)??null}));
const clockPaths=['v1','v3'].map(input=>({input,output:clock.getTimePathCandidates(input)}));
const {schemas,sample}=require('./oracle.cjs');
const encode=value=>JSON.parse(JSON.stringify(value,(_,v)=>typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0))?{number:Object.is(v,-0)?'-0':String(v)}:v));
const versionStates=[];
for(const seed of [0,-0,0.5,-1,Number.MAX_SAFE_INTEGER-1,Number.MAX_SAFE_INTEGER,2**53,1e100,NaN,Infinity,-Infinity]) {
 o.positionStateVersion=seed;o.balanceStateVersion=-seed;
 const steps=[];
 for(const method of ['getPositionStateVersion','getBalanceStateVersion','markPositionStateUpdatedByWebsocket','markBalanceStateUpdatedByWebsocket','getPositionStateVersion','getBalanceStateVersion','markBalanceStateUpdatedByWebsocket','markPositionStateUpdatedByWebsocket']){
  const returned=o[method]();steps.push({method,returned:returned===undefined?{undefined:true}:encode(returned),position:encode(o.positionStateVersion),balance:encode(o.balanceStateVersion)});
 }
 versionStates.push({position:encode(seed),balance:encode(-seed),steps});
}
const tickers=[];for(const text of ['12.5','0x10','12abc','','NaN','Infinity','-Infinity','-0'])for(const oi of [undefined,0,7,NaN,Infinity]){const input={...sample(schemas.AsterDEXTickerResponse),symbol:'BTC',lastPrice:text,priceChangePercent:text,volume:text,quoteVolume:text,unknownPayload:{nested:[false,null]}};const map=oi===undefined?undefined:new Map([['BTC',oi]]);tickers.push({input,oi:oi===undefined?null:encode(oi),output:encode(o.transformTicker(input,map))});}
const positions=[];for(const positionSide of ['BOTH','','LONG','SHORT','long'])for(const positionAmt of ['0','-0','12abc','-2','NaN','Infinity','0x10',''])for(const leverage of ['','0','-5','bad','Infinity'])for(const marginType of ['ISOLATED','isolated','CROSSED']){const input={...sample(schemas.AsterDEXPositionResponse),symbol:'BTC',positionSide,positionAmt,leverage,marginType,entryPrice:'21.5',notional:'31.75',unRealizedProfit:'-4.25',liquidationPrice:'43.5',unknownPayload:{nested:[false,null]}};const stored=[{id:'BTC',realizedPnl:9},{id:'BTC:long',realizedPnl:4},{id:'BTC:short'},{id:'BTC:long',realizedPnl:999}];o.store={positions:stored};positions.push({input,stored,output:encode(o.transformPosition(input))});}
const limits=[undefined,null,{}, {filters:null},{filters:{}},{filters:[]},{filters:[null]},{filters:[{filterType:'OTHER'},null]}];
for(const limit of [undefined,null,false,true,0,-1,'2','0x10','12abc','Infinity',[],[4],{toString:null}])limits.push({filters:[{filterType:'MAX_NUM_ALGO_ORDERS',...(limit===undefined?{}:{limit})},null]});
const algoLimits=limits.map(info=>({info,output:result(()=>o.getMaxAlgoOrdersLimit(info===undefined?undefined:{info}))}));
const counts=[];for(const status of ['open','closed','canceled','creating'])for(const type of ['limit','stop_market','take_profit_market'])for(const rawType of ['TRAILING_STOP_MARKET','LIMIT','TRAILING_STOP']){const orders=[{id:'1',symbol:'BTC',status,type,side:'buy',price:1,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:1,info:{type:rawType}},{id:'2',symbol:'ETH',status:'open',type:'stop_market',side:'sell',price:1,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:1}];o.store={orders};counts.push({orders,output:o.getOpenAlgoOrdersCount('BTC')});}
const capacities=[];for(const info of limits)for(const added of [-1,0,-0,0.5,1,2,1e21,NaN,Infinity])for(const orders of [[],counts[0].orders]){o.store={marketsBySymbol:info===undefined?{}:{BTC:{info}},orders};capacities.push({info,added:encode(added),orders,output:result(()=>{o.assertAlgoOrderCapacity('BTC',added);return null})});}
const ticks=[];const filterCases=[undefined,null,false,0,'',{},'x',[],[null],[false,0,'x'],[{filterType:'OTHER'},null]];
for(const tickSize of [undefined,null,false,true,0,-0,1,1e-7,'0','0x10','12abc','Infinity','-0',[],[2],{}, {toString:null}])filterCases.push([{filterType:'PRICE_FILTER',...(tickSize===undefined?{}:{tickSize})},{filterType:'PRICE_FILTER',tickSize:'999'},null]);
for(const filters of filterCases){const market={precision:{price:0.25},info:{...(filters===undefined?{}:{filters})}};ticks.push({market,output:encode(result(()=>o.getTickSize(market)))});}
const orderTypes=['market','limit','conditional','stop_market','take_profit_market','trailing_stop_market'].map(input=>({input,output:result(()=>o.mapOrderType(input))}));
const timeframes=['1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w'].map(input=>({input,output:o.mapTimeframe(input)}));
const canSign=[];for(const version of ['v1','v3'])for(const remote of [false,true])for(const key of ['', ' ', 'x', '0x'+'a'.repeat(40)])for(const secret of ['', ' ', 'a'.repeat(64)])for(const auth of [undefined,{userAddress:'0x'+'b'.repeat(40),signerAddress:'0x'+'c'.repeat(40),privateKey:'d'.repeat(64)}]){const options={key,secret,auth};o.options={...options,...(remote?{remoteSigner:{}}:{})};o.apiVersion=version;canSign.push({options,version,remote,output:o.canSign});}
const paths=[];for(const version of ['v1','v3'])for(const override of [undefined,'v1','v3','v4'])for(const endpoint of ['','/','///order',' /order','/order?x=1','\ufeff/order']){o.apiVersion=version;paths.push({version,override,endpoint,output:o.getFapiPath(endpoint,override),account:o.getAccountEndpoint()});}
const reverseStatuses = [...Object.getOwnPropertyNames(Object.prototype),'NEW','PARTIALLY_FILLED','FILLED','CANCELED','REJECTED','EXPIRED','unknown','','new',' constructor','constructor\ufeff','😀','é','\ufeff','\u0085'].map(input => {
 const value=o.reverseMapOrderStatus(input);
 const inherited=typeof value==='function'||value===Object.prototype;
 const actualKey=inherited?Object.getOwnPropertyNames(Object.prototype).find(key=>({})[key]===value):undefined;
 if(inherited&&actualKey===undefined)throw Error('Unidentified inherited status result');
 const member=actualKey==='__proto__'?'Prototype':actualKey?.replace(/^__|__$/g,'').replace(/^./,c=>c.toUpperCase());
 return {input,output:inherited?{inherited:member,type:typeof value}:{mapped:value},serialized:JSON.stringify({status:value})};
});
const reverseStatusSurrogates = ['"\\ud800"','"a\\udfff"'].map(inputJson=>({inputJson,output:o.reverseMapOrderStatus(JSON.parse(inputJson))}));
const reverseTypes = [...Object.getOwnPropertyNames(Object.prototype),'LIMIT','MARKET','STOP','STOP_MARKET','TAKE_PROFIT','TAKE_PROFIT_MARKET','TRAILING_STOP','TRAILING_STOP_MARKET','alien','SELL','',' limit','😀','é','\ufeff','\u0085'].map(input=>{
 const value=o.reverseMapOrderType(input);
 const inherited=typeof value==='function'||value===Object.prototype;
 const key=inherited?Object.getOwnPropertyNames(Object.prototype).find(key=>({})[key]===value):undefined;
 if(inherited&&key===undefined)throw Error('Unidentified inherited order type');
 return {input,output:inherited?{inherited:key==='__proto__'?'Prototype':key.replace(/^__|__$/g,'').replace(/^./,c=>c.toUpperCase()),type:typeof value}:{mapped:value},serialized:JSON.stringify({type:value})};
});
const filterValues=[];
for(const field of [...Object.getOwnPropertyNames(Object.prototype),'minQty','missing','😀','é','\ufeff','\u0085'])
 for(const value of [undefined,null,false,true,0,-0,1,-2,'','0','-0','12abc','0x10','Infinity','\ufeff12','\u008512',[],[2],{}, {toString:null}]){
  const first={filterType:'LOT_SIZE'};if(value!==undefined)Object.defineProperty(first,field,{value,enumerable:true});
  for(const filters of [[],[null,first],[false,0,'str',first,{filterType:'LOT_SIZE',[field]:'999'},null],[{filterType:'OTHER'},first]])
   filterValues.push({filters,field,output:encode(result(()=>o.extractFilterValue(filters,'LOT_SIZE',field)))});
 }
const lookupSurrogates=['"\\ud800"','"a\\udfff"'].map(inputJson=>{const input=JSON.parse(inputJson);return {inputJson,typeOutputJson:JSON.stringify(o.reverseMapOrderType(input)),filterOutput:o.extractFilterValue([{filterType:input,value:'3'}],input,'value')}});
const markets=[];
for(const precision of [-3,-0.1,0,0.1,1,1.5,2,8,20,308,324])
 for(const leverage of [undefined,0,-0,NaN,-5,0.5,10,Infinity,-Infinity])
  for(const field of [undefined,'','0','-0','12abc','0x10','Infinity','\ufeff12','\u008512']) {
   const input={...sample(schemas.AsterDEXMarketResponse),symbol:'BTC',pricePrecision:precision,quantityPrecision:precision,status:precision===1?'TRADING':'trading',filters:[{filterType:'OTHER',extraFilter:{nested:[false,null]}},{filterType:'LOT_SIZE',...(field===undefined?{}:{minQty:field,maxQty:field})},{filterType:'LOT_SIZE',minQty:'999',maxQty:'999'}],unknownPayload:{nested:[false,null]}};
   const brackets=leverage===undefined?new Map():new Map([['BTC',leverage]]);
   markets.push({input,leverage:leverage===undefined?null:encode(leverage),output:encode(o.transformMarket(input,brackets))});
  }
for(const symbol of ['é','😀','\ufeff','\u0085']) {
 const input={...sample(schemas.AsterDEXMarketResponse),symbol,baseAsset:symbol,quoteAsset:symbol,pricePrecision:2,quantityPrecision:2,filters:[]};
 markets.push({input,leverage:null,output:encode(o.transformMarket(input,new Map()))});
}
const marketSurrogates=['"\\ud800"','"a\\udfff"'].map(symbolJson=>{
 const input={...sample(schemas.AsterDEXMarketResponse),symbol:JSON.parse(symbolJson),pricePrecision:2,quantityPrecision:2,filters:[]};
 return {inputJson:JSON.stringify(input),symbolJson,outputJson:JSON.stringify(o.transformMarket(input,new Map()))};
});
const marketNonfiniteInfo=[NaN,-Infinity].map(value=>{
 const input={...sample(schemas.AsterDEXMarketResponse),symbol:'BTC',pricePrecision:value,quantityPrecision:0,filters:[]};
 return {input:encode(input),precision:encode(value),output:encode(o.transformMarket(input,new Map()))};
});
const numericInfo=[];
for(const value of [NaN,Infinity,-Infinity])for(const kind of ['ticker','position']) {
 const field=kind==='ticker'?'closeTime':'updateTime';
 const input={...sample(schemas[kind==='ticker'?'AsterDEXTickerResponse':'AsterDEXPositionResponse']),symbol:'BTC',[field]:value};
 o.store={positions:[]};const output=kind==='ticker'?o.transformTicker(input):o.transformPosition(input);
 numericInfo.push({kind,field,input:encode(input),raw:encode(output.info[field])});
}
const mapperStrings=[];
for(const [kind,schema] of [['ticker','AsterDEXTickerResponse'],['position','AsterDEXPositionResponse']])
 for(const symbolJson of ['"\\ud800"','"a\\udfff"',...['é','😀','\ufeff','\u0085'].map(JSON.stringify)]) {
  const input={...sample(schemas[schema]),symbol:JSON.parse(symbolJson)};
  o.store={positions:[]};
  const output=kind==='ticker'?o.transformTicker(input):o.transformPosition(input);
  mapperStrings.push({kind,inputJson:JSON.stringify(input),symbolJson,outputJson:JSON.stringify(output)});
 }
const orders=[];
const orderInput=(patch={})=>({...sample(schemas.AsterDEXOrderResponse),symbol:'BTC',orderId:123,type:'LIMIT',status:'NEW',side:'BUY',price:'17',origQty:'3',executedQty:'1',unknownPayload:{nested:[false,null]},...patch});
for(const type of ['LIMIT','MARKET','STOP','STOP_MARKET','TAKE_PROFIT','TAKE_PROFIT_MARKET','TRAILING_STOP','TRAILING_STOP_MARKET','limit','market','conditional'])
 for(const stopPrice of [undefined,'','0','-1','-0','0x10','12junk','Infinity','NaN','\ufeff12','\u008512'])
  for(const flag of [undefined,false,true]) {
   const input=orderInput({type,side:flag===true?'BUY':flag===false?'SELL':'buy',stopPrice,closePosition:flag,reduceOnly:flag===false,status:flag===true?'FILLED':flag===false?'unknown':'NEW'});
   orders.push({input,output:encode(o.transformOrder(input))});
  }
for(const value of ['','-0','1e309','-1e309','bad','0x10','1e-300','12junk']) {
 const input=orderInput({price:value,origQty:value,executedQty:value});orders.push({input,output:encode(o.transformOrder(input))});
}
for(const orderId of [0,-1,1e-7,1e20,1e21,9007199254740992]) {
 const input=orderInput({orderId});orders.push({input,output:encode(o.transformOrder(input))});
}
const orderRefusals=[];
for(const field of ['type','status'])for(const key of Object.getOwnPropertyNames(Object.prototype)) {
 const input=orderInput({[field]:key});const output=o.transformOrder(input);
 orderRefusals.push({input,field,key,typeof:typeof output[field],coercion:String(output[field]),serialized:JSON.stringify(output)});
}
for(const key of ['ALIEN','','SELL','Limit']) {
 const input=orderInput({type:key});const output=o.transformOrder(input);
 orderRefusals.push({input,field:'type',key,typeof:typeof output.type,coercion:String(output.type),serialized:JSON.stringify(output)});
}
const orderNumericInfo=[];
for(const field of ['orderId','time','updateTime','workingTime'])for(const value of [NaN,Infinity,-Infinity,-0]) {
 const input=orderInput({[field]:value});orderNumericInfo.push({input:encode(input),field,raw:encode(value),output:encode(o.transformOrder(input))});
}
const numericMetadata=[];
for(const [kind,schema] of [['ticker','AsterDEXTickerResponse'],['position','AsterDEXPositionResponse'],['market','AsterDEXMarketResponse'],['order','AsterDEXOrderResponse']]) {
 const fields=schemas[schema].fields.filter(f=>f.shape.kind==='number').map(f=>'/'+f.name);
 if(kind==='market') fields.push('/filters/0/limit');
 for(const pointer of fields)for(const value of [-0,1.25,Infinity,-Infinity,NaN]) {
  if(kind==='market'&&['/pricePrecision','/quantityPrecision'].includes(pointer)&&value===Infinity)continue;
  const input=kind==='order'?orderInput():{...sample(schemas[schema]),symbol:'BTC'};
  if(kind==='market'){input.filters=[];input.pricePrecision=2;input.quantityPrecision=2;}
  if(pointer==='/filters/0/limit')input.filters=[{filterType:'MAX_NUM_ALGO_ORDERS',limit:value}];
  else input[pointer.slice(1)]=value;
  o.store={positions:[]};
  const output=kind==='ticker'?o.transformTicker(input):kind==='position'?o.transformPosition(input):kind==='market'?o.transformMarket(input,new Map()):o.transformOrder(input);
  const native=pointer.split('/').slice(1).reduce((v,k)=>v[k],output.info);
  if(typeof native!=='number'||!Object.is(native,value))throw Error('Source native metadata changed '+kind+pointer);
  const bits=Buffer.alloc(8);bits.writeDoubleBE(native);
  numericMetadata.push({kind,pointer,input:encode(input),raw:encode(native),bits:bits.toString('hex')});
 }
}
const orderOverflowIngress=[];
for(const field of ['orderId','time','updateTime','workingTime'])for(const literal of ['1e400','-1e400']) {
 const baseline=orderInput({[field]:12345});
 const inputJson=JSON.stringify(baseline).replace('"'+field+'":12345','"'+field+'":'+literal);
 const input=JSON.parse(inputJson);
 if(Number.isFinite(input[field]))throw Error('overflow ingress source missing');
 orderOverflowIngress.push({field,inputJson,raw:encode(input[field]),output:encode(o.transformOrder(input))});
}
const orderSignedZeroIngress=['orderId','time','updateTime','workingTime'].map(field=>{
 const inputJson=JSON.stringify(orderInput({[field]:12345})).replace('"'+field+'":12345','"'+field+'":-0');
 const value=JSON.parse(inputJson)[field];const bits=Buffer.alloc(8);bits.writeDoubleBE(value);
 return {captureArchitecture:`${process.platform}-${process.arch}`,captureNodeMajor:Number(process.versions.node.split('.')[0]),field,inputJson,bits:bits.toString('hex')};
});
const orderStrings=['"\\ud800"','"a\\udfff"',...['é','😀','\ufeff','\u0085'].map(JSON.stringify)].map(symbolJson=>{
 const input=orderInput({symbol:JSON.parse(symbolJson)});return {inputJson:JSON.stringify(input),symbolJson,outputJson:JSON.stringify(o.transformOrder(input))};
});
const orderAliasInput=orderInput();const orderAliasOutput=o.transformOrder(orderAliasInput);
const orderAlias={input:JSON.parse(JSON.stringify(orderAliasInput)),sameReference:orderAliasInput===orderAliasOutput.info};
orderAliasInput.symbol='SOURCE_AFTER';orderAlias.afterSource={source:orderAliasInput.symbol,info:orderAliasOutput.info.symbol,symbol:orderAliasOutput.symbol};
orderAliasOutput.info.symbol='INFO_AFTER';orderAlias.afterInfo={source:orderAliasInput.symbol,info:orderAliasOutput.info.symbol,symbol:orderAliasOutput.symbol};
// Execute the actual shared emitter too: its formatted event differs from the
// raw message passed by the venue guard, while the thrown Error keeps raw text.
const baseAst=ts.createSourceFile('base.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/base.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const baseClass=baseAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseExchange');
const emitMember=baseClass.members.find(n=>n.name?.getText(baseAst)==='emitAndLogError');
if(!emitMember)throw Error('Missing actual BaseExchange.emitAndLogError');
const contextAst=ts.createSourceFile('httpFailureContext.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/httpFailureContext.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const contextSource=contextAst.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,contextAst)).join('\n');
const emitterSource=contextSource+'\nclass CoreEmitter { '+printer.printNode(ts.EmitHint.Unspecified,emitMember,baseAst)+' }\nexport {CoreEmitter};';
const emitterExports={};let featureCalls=[];
new Function('exports','reportException','console',ts.transpileModule(emitterSource,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(emitterExports,error=>featureCalls.push(['report',error.name,error.message]),{error(){},trace(){},warn(){}});
const featureGuards=[];
for(const version of ['v1','v3'])for(const feature of ['', 'self-trade prevention','per-order self-trade prevention','é','😀','\ufeff','\u0085','line\nbreak']) {
 o.apiVersion=version;const calls=[];featureCalls=calls;o.name='asterdex';o.options={key:'',secret:''};o.isDisposed=false;o.emitter={emit:(event,message)=>calls.push(['emit',message])};o.emitAndLogError=emitterExports.CoreEmitter.prototype.emitAndLogError;
 const output=result(()=>o.assertV3Feature(feature));if(output.error)calls.push(['throw',output.error.name,output.error.message]);
 featureGuards.push({version,feature,calls,success:!output.error});
}
const featureSurrogates=['"\\ud800"','"a\\udfff"'].map(inputJson=>{
 o.apiVersion='v1';const calls=[];featureCalls=calls;o.emitter={emit:(event,message)=>calls.push(['emit',message])};o.emitAndLogError=emitterExports.CoreEmitter.prototype.emitAndLogError;
 const output=result(()=>o.assertV3Feature(JSON.parse(inputJson)));return {inputJson,outputJson:JSON.stringify(output),callsJson:JSON.stringify(calls)};
});
const accountModes=[false,true].map(isHedged=>{o.store={options:{isHedged}};return {isHedged,output:o.getAccountPositionMode()};});
const aliases=[];
for(const [kind,schema] of [['market','AsterDEXMarketResponse'],['ticker','AsterDEXTickerResponse'],['position','AsterDEXPositionResponse']]) {
 const input={...sample(schemas[schema]),symbol:'BTC'};
 if(kind==='market')Object.assign(input,{pricePrecision:2,quantityPrecision:2,filters:[]});
 const original=JSON.parse(JSON.stringify(input));
 o.store={positions:[]};
 const output=kind==='market'?o.transformMarket(input,new Map()):kind==='ticker'?o.transformTicker(input):o.transformPosition(input);
 const sameReference=output.info===input;
 input.symbol='SOURCE_AFTER';
 const afterSource={source:input.symbol,info:output.info.symbol,symbol:output.symbol};
 output.info.symbol='INFO_AFTER';
 aliases.push({kind,input:original,sameReference,afterSource,afterInfo:{source:input.symbol,info:output.info.symbol,symbol:output.symbol}});
}
async function openInterestCases() {
 const inputs=[
  {name:'bulk succeeds',symbols:['BTC'],allow:true,queue:[{data:[{symbol:'BTC',openInterest:'2'},{symbol:'BTC',openInterest:'3'},{symbol:'NEGZERO',openInterest:'-0'},{symbol:'ETH',openInterest:'Infinity'}]},{data:{symbol:'BTC',openInterest:'3'}}]},
  {name:'bulk failure fallback continues',symbols:['','BTC','ETH','BTC'],allow:true,queue:[new Error('bulk down'),{data:{symbol:'BTC',openInterest:'4'}},new Error('one symbol down'),{data:{symbol:'BTC',openInterest:'5'}}]},
  {name:'bulk empty fallback disabled',symbols:['BTC'],allow:false,queue:[{data:[]}]},
  {name:'empty body remains non-object',symbols:['BTC'],allow:false,queue:[{data:''}]},
  {name:'bulk empty symbols absent',allow:true,queue:[{data:[]}]},
  {name:'coercion error keeps partial map',symbols:['BTC'],allow:true,queue:[{data:[{symbol:'BTC',openInterest:'1'},{symbol:'ETH',openInterest:{toString:null}}]},{data:{symbol:'BTC',openInterest:'2'}}]},
  {name:'bulk nonempty suppresses fallback',symbols:['ETH'],allow:true,queue:[{data:{data:{symbol:'BTC',openInterest:0}}}]},
 ];
 const results=[];
 for(const input of inputs) {
  const requests=[],logs=[],queue=[...input.queue];o.apiVersion='v1';o.publicAPI={get:async (url,config={})=>{requests.push({url,config});const next=queue.shift();if(next instanceof Error)throw next;return next;}};o.log=(message,severity)=>logs.push({message,severity});
  const map=await o.fetchOpenInterestMap({symbols:input.symbols,allowPerSymbolFallback:input.allow});
  results.push({name:input.name,symbols:input.symbols??null,allow:input.allow,responses:input.queue.map(item=>item instanceof Error?{error:item.message}:item),requests,logs,entries:[...map].map(([key,value])=>[key,typeof value==='number'&&(!Number.isFinite(value)||Object.is(value,-0))?{number:Object.is(value,-0)?'-0':String(value)}:value]),unconsumed:queue.length});
 }
 const inputJson='{"symbol":"\\ud800","openInterest":1}';
 o.publicAPI={get:async()=>({data:JSON.parse(inputJson)})};o.log=()=>{};
 const loneMap=await o.fetchOpenInterestMap({allowPerSymbolFallback:false});
 return {cases:results,loneSurrogate:{inputJson,typeof:typeof [...loneMap.keys()][0],size:loneMap.size}};
}
openInterestCases().then(fetchOpenInterest=>process.stdout.write(JSON.stringify({captureArchitecture:`${process.platform}-${process.arch}`,captureNodeMajor:Number(process.versions.node.split('.')[0]),numericMetadata,versionStates,orderSignedZeroIngress,featureGuards,featureSurrogates,orderOverflowIngress,orders,orderRefusals,orderNumericInfo,orderStrings,orderAlias,accountModes,mapperStrings,aliases,numericInfo,markets,marketNonfiniteInfo,marketSurrogates,reverseTypes,filterValues,lookupSurrogates,reverseStatuses,reverseStatusSurrogates,side,interest,algo,unknownErrors,tif,serverTimes,clockPaths,tickers,positions,algoLimits,counts,capacities,ticks,orderTypes,timeframes,canSign,paths,fetchOpenInterest:fetchOpenInterest.cases,fetchOpenInterestLoneSurrogate:fetchOpenInterest.loneSurrogate})));
