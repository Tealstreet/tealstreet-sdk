const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const {preserveSpecialNumbers}=require('./special-numbers.cjs');
const {captureHeader}=require('./special-numbers.cjs');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const {indexedStringLiterals}=require('./literal-tuples.cjs');
const sharedOrderFixturePath=path.resolve(__dirname,'../../../packages/safe-cex/src/__tests__/audit-regressions/coinbase.starbase-private.fixtures.ts');
const sharedOrderFixtures={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(sharedOrderFixturePath,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:sharedOrderFixtures});
const source=ts.createSourceFile('private.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-private.ts'),'utf8'));
const names=['MarginModel','OrderDirection','OrderState','OrderType','TimeInForce','TradeState','TradeType','TriggerType','FeeCurrency','ParityBlocker'].map(name=>'CoinbaseStarbase'+name);
const exportsObject={};
const tables=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(source).startsWith('STARBASE_')));
const tableNames=tables.flatMap(n=>Array.from(n.declarationList.declarations).map(d=>d.name.getText(source)));
vm.runInNewContext(ts.transpileModule(tables.map(n=>n.getText(source)).join('\n')+'\nexport {'+tableNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject});
function literals(node){
 if(ts.isIndexedAccessTypeNode(node)){
  const {name,values}=indexedStringLiterals(ts,source,node);
  assert.deepEqual(Array.from(exportsObject[name]),values,'executed private tuple matches declared literals');
  return values;
 }
 if(ts.isUnionTypeNode(node))return node.types.flatMap(literals);
 if(ts.isLiteralTypeNode(node)&&ts.isStringLiteral(node.literal))return [node.literal.text];
 throw Error('unsupported private literal declaration '+node.getText(source));
}
const aliases=source.statements.filter(ts.isTypeAliasDeclaration);
const observed=aliases.filter(n=>ts.isIndexedAccessTypeNode(n.type)||(ts.isUnionTypeNode(n.type)&&n.type.types.every(t=>ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal)))).map(n=>n.name.text);
assert.deepEqual(observed,names,'all private literal declarations classified');
const schemas=Object.fromEntries(names.map(name=>[name,{kind:'enum',values:literals(aliases.find(n=>n.name.text===name).type)}]));
function recordShape(node){
 if(node.kind===ts.SyntaxKind.StringKeyword)return {kind:'string'};
 if(node.kind===ts.SyntaxKind.NumberKeyword)return {kind:'number'};
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return {kind:'boolean'};
 if(node.kind===ts.SyntaxKind.UnknownKeyword)return {kind:'unknown'};
 if(ts.isTypeReferenceNode(node)){
  const name=node.typeName.getText(source);
  if(['Method','Params','T','Gateway'].includes(name))return {kind:'parameter',name};
  if(name==='Record'){assert.equal(node.typeArguments.length,2);return {kind:'record',key:recordShape(node.typeArguments[0]),value:recordShape(node.typeArguments[1])};}
  return recordShape(aliases.find(n=>n.name.text===name).type);
 }
 if(ts.isArrayTypeNode(node))return {kind:'array',item:recordShape(node.elementType)};
 if(ts.isUnionTypeNode(node)&&!node.types.every(t=>ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal)))return {kind:'union',variants:node.types.map(recordShape)};
 if(ts.isTypeLiteralNode(node))return {kind:'object',fields:node.members.map(field=>({name:field.name.text,optional:!!field.questionToken,schema:recordShape(field.type)}))};
 return {kind:'enum',values:literals(node)};
}
const recordNames=['CoinbaseStarbasePrivateRequest','CoinbaseStarbaseAccountSummary','CoinbaseStarbasePrivatePosition','CoinbaseStarbaseOrderHistoryOptions','CoinbaseStarbaseOrderHistoryPageParams','CoinbaseStarbasePrivateOrder','CoinbaseStarbasePrivateTrade','CoinbaseStarbaseTradePage','CoinbaseStarbaseParityResult'];
assert.deepEqual(aliases.map(n=>n.name.text).sort(),[...names,...recordNames].sort(),'all private type declarations covered');
const recordSchemas=Object.fromEntries(recordNames.map(name=>{
 const node=aliases.find(n=>n.name.text===name);
 return [name,{...recordShape(node.type),...(node.typeParameters?{parameters:node.typeParameters.map(p=>({name:p.name.text,constraint:p.constraint?recordShape(p.constraint):null,default:p.default?recordShape(p.default):null}))}:{})}];
}));
const stateClass = source.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'CoinbaseStarbaseReadOnlyState');
assert(stateClass, 'read-only state class declaration');
recordSchemas.CoinbaseStarbaseReadOnlyState = {kind:'object',fields:stateClass.members.filter(ts.isPropertyDeclaration).map(field=>({name:field.name.getText(source),optional:!!field.questionToken,schema:recordShape(field.type)}))};
schemas.CoinbaseStarbaseAccountCurrency=recordSchemas.CoinbaseStarbaseAccountSummary.fields.find(f=>f.name==='currency').schema;
schemas.CoinbaseStarbasePositionDirection=recordSchemas.CoinbaseStarbasePrivatePosition.fields.find(f=>f.name==='direction').schema;
schemas.CoinbaseStarbaseMarketPrice=recordSchemas.CoinbaseStarbasePrivateOrder.fields.find(f=>f.name==='price').schema.variants[1];
schemas.CoinbaseStarbaseTradeLiquidity=recordSchemas.CoinbaseStarbasePrivateTrade.fields.find(f=>f.name==='liquidity').schema;
function sample(schema,mask,choice=0){
 switch(schema.kind){
  case 'string':return 'value\uFEFF';
  case 'number':return 12.25;
  case 'boolean':return false;
  case 'enum':return schema.values[0];
  case 'parameter':assert(['Method','Params','T','Gateway'].includes(schema.name));return schema.name==='Method'?'private/test':{value:false};
  case 'union':return sample(schema.variants[choice],mask,choice);
  case 'array':return [sample(schema.item,mask,choice)];
  case 'object':{let optional=0;return Object.fromEntries(schema.fields.filter(field=>!field.optional||!!(mask&(1<<optional++))).map(field=>[field.name,sample(field.schema,mask,choice)]));}
  default:throw Error('unclassified record sample '+schema.kind);
 }
}
const recordCases=recordNames.filter(name=>!['CoinbaseStarbasePrivateRequest','CoinbaseStarbaseParityResult'].includes(name)).flatMap(name=>{
 const schema=recordSchemas[name],count=schema.fields.filter(field=>field.optional).length;
 return Array.from({length:2**count},(_,mask)=>(name==='CoinbaseStarbasePrivateOrder'?[0,1]:[0]).map(choice=>({name,mask,choice,value:sample(schema,mask,choice)}))).flat();
});
const parityCases=recordSchemas.CoinbaseStarbaseParityResult.variants.map((variant,index)=>({index,value:sample(variant,0)}));
const requestSample=sample(recordSchemas.CoinbaseStarbasePrivateRequest,0);
const guardNames=['isRecord','protocolError','requireRecord','requireArray','requireString','optionalString','requireFinite','requireNonNegative','requirePositive','requireTimestamp','requirePositiveInteger','optionalBoolean','parseEnum','validateCount'];
const guards=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>guardNames.includes(d.name.getText(source))));
assert.equal(guards.length,guardNames.length,'all private guard implementations selected');
const protocol={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:protocol});
vm.runInNewContext(ts.transpileModule(guards.map(n=>n.getText(source)).join('\n')+'\nexport {'+guardNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject,CoinbaseStarbaseProtocolError:protocol.CoinbaseStarbaseProtocolError});
const values=[undefined,null,false,true,0,-0,-1,1,1.5,-1.5,1e-30,1e30,Number.MIN_VALUE,Number.MAX_VALUE,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,-Number.MAX_SAFE_INTEGER,999,1000,1001,1000.1,'',' ','1','true','0x10','constructor','__proto__','toString','\uFEFF','\u0085','💰',[],[1],{}, {z:1,a:2}, {value:null}];
const contexts=['field','','\uFEFF','\u0085','💰','line\nbreak'];
const guardCases=[];
function guardCapture(fn){try{const value=fn();return value===undefined?{undefined:true}:preserveSpecialNumbers({ok:value});}catch(error){return {error:{name:error.name,message:error.message}};}}
const inputJson=value=>value===undefined?{}:{inputJson:Object.is(value,-0)?'-0':JSON.stringify(value)};
for(const name of guardNames.filter(name=>!['protocolError','parseEnum'].includes(name)))for(const value of values)for(const context of contexts)for(const allowEmpty of name==='requireString'?[undefined,false,true]:[undefined]){
 const result=guardCapture(()=>{
  const output=exportsObject[name](value,context,allowEmpty);
  if(name==='requireRecord'||name==='requireArray')assert.equal(output,value,'guard must retain input identity');
  return output;
 });
 guardCases.push({name,...inputJson(value),context,...allowEmpty===undefined?{}:{allowEmpty},result});
}
for(const value of Array.from({length:1004},(_,i)=>i-1))for(const context of contexts)guardCases.push({name:'validateCount',...inputJson(value),context,result:guardCapture(()=>exportsObject.validateCount(value,context))});
const enumSets=[...names.filter(name=>name!=='CoinbaseStarbaseParityBlocker').map(name=>schemas[name].values),[],[''],['constructor','__proto__','toString'],['buy','buy'],['\uFEFF','\u0085','é','💰']];
for(const allowed of enumSets)for(const value of [...values,...allowed])for(const context of contexts)guardCases.push({name:'parseEnum',...inputJson(value),context,allowed,result:guardCapture(()=>exportsObject.parseEnum(value,allowed,context))});
for(const context of contexts)guardCases.push({name:'protocolError',context,result:guardCapture(()=>exportsObject.protocolError(context))});
const builderNames=['buildOrderHistoryPageParams','buildCoinbaseStarbaseAccountSummary','buildCoinbaseStarbasePositions','buildCoinbaseStarbaseOpenOrders','buildCoinbaseStarbaseOrderState','buildCoinbaseStarbaseOrderHistoryByCurrency','buildCoinbaseStarbaseOrderHistoryByInstrument','buildCoinbaseStarbaseUserTrades','buildCoinbaseStarbaseUserTradesByTime'];
const publicSource=ts.createSourceFile('public.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-public.ts'),'utf8'));
const publicNames=['STARBASE_BASE_CURRENCIES','parseBaseCurrency','parseStarbasePerpetualName','toCoinbaseStarbaseInstrumentName','toCoinbaseLegacyIntxSymbol','requireString','protocolError'];
const publicDefinitions=publicSource.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>publicNames.includes(d.name.getText(publicSource))));
assert.equal(publicDefinitions.length,publicNames.length);
const publicExports={};
vm.runInNewContext(ts.transpileModule(publicDefinitions.map(n=>n.getText(publicSource)).join('\n')+'\nexport {'+publicNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:publicExports,CoinbaseStarbaseProtocolError:protocol.CoinbaseStarbaseProtocolError});
const builders=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>builderNames.includes(d.name.getText(source))));
assert.equal(builders.length,builderNames.length);
vm.runInNewContext(ts.transpileModule(builders.map(n=>n.getText(source)).join('\n')+'\nexport {'+builderNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...exportsObject,toCoinbaseStarbaseInstrumentName:publicExports.toCoinbaseStarbaseInstrumentName,exports:exportsObject});
const builderCases=[];
for(const name of ['buildCoinbaseStarbaseAccountSummary','buildCoinbaseStarbasePositions','buildCoinbaseStarbaseOpenOrders'])builderCases.push({name,result:guardCapture(()=>exportsObject[name]())});
const arg=(name,value)=>value===undefined?{}:{[name]:Object.is(value,-0)?'-0':JSON.stringify(value)};
for(const orderId of values)builderCases.push({name:'buildCoinbaseStarbaseOrderState',...arg('orderIdJson',orderId),result:guardCapture(()=>exportsObject.buildCoinbaseStarbaseOrderState(orderId))});
for(const count of values)builderCases.push({name:'buildCoinbaseStarbaseUserTrades',...arg('countJson',count),result:guardCapture(()=>exportsObject.buildCoinbaseStarbaseUserTrades(count))});
const historyInputs=[undefined,{}, {unknown:true}, {count:null}];
const optionKeys=['count','offset','includeOld','includeUnfilled','historical'];
for(const key of optionKeys)for(const value of values.filter(v=>!Object.is(v,-0)))historyInputs.push({[key]:value});
for(const a of optionKeys)for(const b of optionKeys)if(a!==b)for(const bad of [null,[],{},'invalid'])historyInputs.push({[a]:bad,[b]:bad});
for(let mask=0;mask<3125;mask++){
 let state=mask;
 const options={};
 for(const key of optionKeys){options[key]=[undefined,null,false,true,0][state%5];state=Math.floor(state/5);}
 historyInputs.push(options);
}
const symbols=['BTC-PERP-INTX','ETH_USDC-PERPETUAL','','SOL-PERP-INTX','BTC-PERP-INTX\n','\uFEFFBTC-PERP-INTX'];
function historyCases(options,raw){
 const names=['buildOrderHistoryPageParams','buildCoinbaseStarbaseOrderHistoryByCurrency','buildCoinbaseStarbaseOrderHistoryByInstrument'];
 for(const name of names){
  if(name==='buildOrderHistoryPageParams'&&options===undefined)continue;
  for(const symbol of name==='buildCoinbaseStarbaseOrderHistoryByInstrument'?symbols:[undefined]){
   const result=guardCapture(()=>name==='buildCoinbaseStarbaseOrderHistoryByInstrument'?exportsObject[name](symbol,options):exportsObject[name](options));
   builderCases.push({name,...symbol===undefined?{}:{symbol},...options===undefined?{}:{optionsJson:raw??JSON.stringify(options)},result});
  }
 }
}
for(const options of historyInputs)historyCases(options);
for(const raw of ['{"offset":-0}','{"offset":-0,"includeOld":false}','{"count":-0}','{"count":null,"offset":-0}'])historyCases(JSON.parse(raw),raw);
const times=[undefined,null,false,true,0,-0,1,2,-1,1.5,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,'1',{},[]];
for(const start of times)for(const end of times)for(const count of values){
 builderCases.push({name:'buildCoinbaseStarbaseUserTradesByTime',...arg('startJson',start),...arg('endJson',end),...arg('countJson',count),result:guardCapture(()=>exportsObject.buildCoinbaseStarbaseUserTradesByTime(start,end,count))});
}
const accountNames=['decodeCoinbaseStarbaseAccountSummary','assessCoinbaseStarbaseBalanceParity'];
const accountDefinitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>accountNames.includes(d.name.getText(source))));
assert.equal(accountDefinitions.length,accountNames.length);
vm.runInNewContext(ts.transpileModule(accountDefinitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+accountNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...exportsObject,exports:exportsObject});
const accountBase={currency:'USDC',balance:11,equity:12,available_funds:13,available_withdrawal_funds:14,initial_margin:15,maintenance_margin:16,futures_pl:17,futures_session_rpl:18,futures_session_upl:19,session_rpl:20,session_upl:21,total_pl:22};
const accountOptional={margin_balance:23,total_equity_usd:24,total_initial_margin_usd:25,total_maintenance_margin_usd:26,total_margin_balance_usd:27,cross_collateral_enabled:false,portfolio_margining_enabled:true,margin_model:'cross_pm'};
const accountAssessment=accountDefinitions.flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='assessCoinbaseStarbaseBalanceParity').initializer;
assert(ts.isTypeReferenceNode(accountAssessment.type));
const accountAssessmentReturn={name:accountAssessment.type.typeName.getText(source),arguments:accountAssessment.type.typeArguments.map(type=>type.getText(source))};
const accountCases=[];
function accountCase(value,raw){
 const result=guardCapture(()=>exportsObject.decodeCoinbaseStarbaseAccountSummary(value));
 let assessment;
 if(result.ok){
  const decoded=exportsObject.decodeCoinbaseStarbaseAccountSummary(value);
  const assessed=exportsObject.assessCoinbaseStarbaseBalanceParity(decoded);
  assert.equal(assessed.gateway,decoded,'assessment preserves account identity');
  assessment=preserveSpecialNumbers(assessed);
 }
 accountCases.push({...value===undefined?{}:{inputJson:raw??JSON.stringify(value)},result,...assessment===undefined?{}:{assessment}});
}
for(const value of values)accountCase(value);
accountCase(accountBase);
const accountFields=Object.keys({...accountBase,...accountOptional});
for(const field of accountFields)for(const value of [...values,'USDC','cross_pm','cross_sm','segregated_pm','segregated_sm']){
 const input={...accountBase,[field]:value};
 accountCase(input,Object.is(value,-0)?JSON.stringify(input).replace('"'+field+'":0','"'+field+'":-0'):undefined);
}
for(const a of accountFields)for(const b of accountFields)if(a!==b)accountCase({...accountBase,[a]:null,[b]:null});
for(let mask=0;mask<256;mask++)for(const model of ['cross_pm','cross_sm','segregated_pm','segregated_sm']){
 const input={...accountBase};
 Object.entries(accountOptional).forEach(([key,value],i)=>{if(mask&(1<<i))input[key]=key==='margin_model'?model:value;});
 accountCase(input);
}
accountCase({...accountBase,unknown:42});
const positionNames=['parseStarbaseInstrument','assertUnique','decodeCoinbaseStarbasePosition','decodeCoinbaseStarbasePositions','assessCoinbaseStarbasePositionParity'];
const positionDefinitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>positionNames.includes(d.name.getText(source))));
assert.equal(positionDefinitions.length,positionNames.length);
vm.runInNewContext(ts.transpileModule(positionDefinitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+positionNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...exportsObject,toCoinbaseLegacyIntxSymbol:publicExports.toCoinbaseLegacyIntxSymbol,exports:exportsObject});
const positionAssessment=positionDefinitions.flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='assessCoinbaseStarbasePositionParity').initializer;
assert(ts.isTypeReferenceNode(positionAssessment.type));
const positionAssessmentReturn={name:positionAssessment.type.typeName.getText(source),arguments:positionAssessment.type.typeArguments.map(type=>type.getText(source))};
const instrumentCases=[];
for(const value of [...values,'BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL','BTC_USDC-PERPETUAL\n','BTC_USDC-PERPETUAL\r','SOL_USDC-PERPETUAL','BTC-PERP-INTX'])for(const context of contexts)instrumentCases.push({...inputJson(value),context,result:guardCapture(()=>exportsObject.parseStarbaseInstrument(value,context))});
const uniqueCases=[];
const ids=['','x','y','constructor','__proto__','\uFEFF','\u0085','é','💰'];
const uniqueLists=[[],...ids.map(id=>[id]),...ids.flatMap(a=>ids.flatMap(b=>ids.map(c=>[a,b,c])))];
for(const list of [...uniqueLists,...uniqueLists.map(list=>list.map(id=>({id,nested:{value:id}})))])for(const failAt of [-1,0,1,2,3]){
 const calls=[];
 const result=guardCapture(()=>exportsObject.assertUnique(list,(value)=>{assert.strictEqual(value,list[calls.length],'callback retains original element');const id=typeof value==='object'?value.id:value;calls.push(id);if(calls.length-1===failAt)throw new protocol.CoinbaseStarbaseProtocolError('callback failure');return id;},'unique context'));
 uniqueCases.push({ids:list,failAt,calls,result});
}
const positionBase={kind:'future',instrument_name:'BTC_USDC-PERPETUAL',direction:'buy',size_currency:2,size:31,mark_price:7,average_price:13,index_price:17,estimated_liquidation_price:19,initial_margin:23,maintenance_margin:29,open_orders_margin:37,floating_profit_loss:41,realized_profit_loss:43,total_profit_loss:47};
const positionCases=[];
function positionCase(value,raw){
 const result=guardCapture(()=>exportsObject.decodeCoinbaseStarbasePosition(value));
 let assessment;
 if(result.ok){const decoded=exportsObject.decodeCoinbaseStarbasePosition(value);const assessed=exportsObject.assessCoinbaseStarbasePositionParity(decoded);assert.equal(assessed.gateway,decoded);assessment=preserveSpecialNumbers(assessed);}
 positionCases.push({...value===undefined?{}:{inputJson:raw??JSON.stringify(value)},result,...assessment===undefined?{}:{assessment}});
}
for(const value of values)positionCase(value);
positionCase(positionBase);
const positionFields=[...Object.keys(positionBase),'leverage'];
for(const field of positionFields)for(const value of [...values,'future','buy','sell','zero','BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL',Number.MAX_VALUE,Number.MIN_VALUE]){
 const input={...positionBase,[field]:value};
 positionCase(input,Object.is(value,-0)?JSON.stringify(input).replace('"'+field+'":0','"'+field+'":-0'):undefined);
}
for(const a of positionFields)for(const b of positionFields)if(a!==b)positionCase({...positionBase,[a]:null,[b]:null});
for(const direction of ['buy','sell','zero'])for(const base of [-2,-0,0,2,Number.MAX_VALUE,Number.MIN_VALUE])for(const size of [-31,-0,0,31]){
 const raw=JSON.stringify({...positionBase,direction,size_currency:base,size}).replace('"size_currency":0','"size_currency":'+(Object.is(base,-0)?'-0':'0')).replace('"size":0','"size":'+(Object.is(size,-0)?'-0':'0'));
 positionCase(JSON.parse(raw),raw);
}
positionCase({...positionBase,size_currency:Number.MAX_VALUE,mark_price:2});
positionCase({...positionBase,unknown:'retained only by source wire'});
const positionsCases=[];
const positionFrames=[undefined,null,false,{},[],[positionBase],[{...positionBase,instrument_name:'ETH_USDC-PERPETUAL'},positionBase],[positionBase,positionBase],[positionBase,positionBase,{...positionBase,mark_price:0}],[{...positionBase,mark_price:0},positionBase,positionBase]];
for(const value of positionFrames)positionsCases.push({...value===undefined?{}:{inputJson:JSON.stringify(value)},result:guardCapture(()=>exportsObject.decodeCoinbaseStarbasePositions(value))});
for(const one of positionCases){if(!one.inputJson)continue;const raw='['+one.inputJson+']';positionsCases.push({inputJson:raw,result:guardCapture(()=>exportsObject.decodeCoinbaseStarbasePositions(JSON.parse(raw)))});}
const orderNames=['parseOrderPrice','decodeCoinbaseStarbaseOrder','decodeCoinbaseStarbaseOpenOrders','decodeCoinbaseStarbaseOrderState','decodeCoinbaseStarbaseOrderHistory','decodeCoinbaseStarbaseOrderHistoryByInstrument'];
const orderDefinitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>orderNames.includes(d.name.getText(source))));
assert.equal(orderDefinitions.length,orderNames.length);
vm.runInNewContext(ts.transpileModule(orderDefinitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+orderNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...exportsObject,toCoinbaseStarbaseInstrumentName:publicExports.toCoinbaseStarbaseInstrumentName,exports:exportsObject});
const priceCases=[...values,'market_price',Number.MAX_VALUE,Number.MIN_VALUE].map(value=>({...inputJson(value),result:guardCapture(()=>exportsObject.parseOrderPrice(value))}));
const sharedOrderIdentityInput={...sharedOrderFixtures.STARBASE_OPEN_ORDER_FIXTURE,post_only:true,reduce_only:false,amount:0.14,filled_amount:0.03,creation_timestamp:1800000000123,last_update_timestamp:1800000000456};
const sharedOrderIdentitySupplement={label:'shared-fixture-field-identity-supplement',input:sharedOrderIdentityInput,result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseOrder(sharedOrderIdentityInput))};
const sharedAccountIdentityInput={...sharedOrderFixtures.STARBASE_ACCOUNT_SUMMARY_FIXTURE,equity:52.5,margin_balance:53.5,futures_pl:1.25,total_pl:2.5,futures_session_rpl:0.3,session_rpl:0.4,futures_session_upl:0.6,session_upl:0.7,cross_collateral_enabled:true,portfolio_margining_enabled:false};
const sharedAccountIdentitySupplement={label:'shared-fixture-field-identity-supplement',input:sharedAccountIdentityInput,result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseAccountSummary(sharedAccountIdentityInput))};
const sharedPositionIdentityInput={...sharedOrderFixtures.STARBASE_POSITION_FIXTURE,leverage:11,realized_profit_loss:12};
const sharedPositionIdentitySupplement={label:'shared-fixture-field-identity-supplement',input:sharedPositionIdentityInput,result:guardCapture(()=>exportsObject.decodeCoinbaseStarbasePosition(sharedPositionIdentityInput))};
const sharedTradeIdentityInput={...sharedOrderFixtures.STARBASE_USER_TRADE_FIXTURE,price:50400,mark_price:50500};
const sharedTradeIdentitySupplement={label:'shared-fixture-field-identity-supplement',input:sharedTradeIdentityInput,result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseTrade(sharedTradeIdentityInput))};
const orderBase={instrument_name:'BTC_USDC-PERPETUAL',post_only:false,order_id:'order-1',order_state:'open',order_type:'limit',time_in_force:'good_til_cancelled',direction:'buy',price:17,label:'',creation_timestamp:101,last_update_timestamp:102};
const orderOptional={average_price:19,reduce_only:false,amount:23,filled_amount:29,contracts:31,trigger:'mark_price',trigger_price:37,trigger_order_id:'trigger-1'};
const orderCases=[];
function orderCase(value,raw){orderCases.push({...value===undefined?{}:{inputJson:raw??JSON.stringify(value)},result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseOrder(value))});}
for(const value of values)orderCase(value);
orderCase(orderBase);
const orderValues=[...values,'market_price','BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL',...schemas.CoinbaseStarbaseOrderState.values,...schemas.CoinbaseStarbaseOrderType.values,...schemas.CoinbaseStarbaseTimeInForce.values,...schemas.CoinbaseStarbaseOrderDirection.values,...schemas.CoinbaseStarbaseTriggerType.values,Number.MAX_VALUE,Number.MIN_VALUE];
const orderFields=Object.keys({...orderBase,...orderOptional});
for(const field of orderFields)for(const value of orderValues){const input={...orderBase,[field]:value};orderCase(input,Object.is(value,-0)?JSON.stringify(input).replace('"'+field+'":0','"'+field+'":-0'):undefined);}
for(const a of orderFields)for(const b of orderFields)if(a!==b)orderCase({...orderBase,[a]:null,[b]:null});
for(let mask=0;mask<256;mask++){const input={...orderBase};Object.entries(orderOptional).forEach(([key,value],i)=>{if(mask&(1<<i))input[key]=value;});orderCase(input);}
for(const state of schemas.CoinbaseStarbaseOrderState.values)for(const type of schemas.CoinbaseStarbaseOrderType.values)for(const time of schemas.CoinbaseStarbaseTimeInForce.values)for(const direction of schemas.CoinbaseStarbaseOrderDirection.values)orderCase({...orderBase,order_state:state,order_type:type,time_in_force:time,direction});
orderCase({...orderBase,unknown:42});
const orderListCases=[];
const orderFrames=[undefined,null,false,{},[],[orderBase],[{...orderBase,order_id:'order-2',instrument_name:'ETH_USDC-PERPETUAL'},orderBase],[orderBase,orderBase],[orderBase,orderBase,{...orderBase,price:-1}],[{...orderBase,price:-1},orderBase,orderBase]];
function orderListCase(value,raw){
 for(const name of ['decodeCoinbaseStarbaseOpenOrders','decodeCoinbaseStarbaseOrderHistory','decodeCoinbaseStarbaseOrderHistoryByInstrument'])for(const symbol of name.endsWith('ByInstrument')?symbols:[undefined]){
  orderListCases.push({name,...value===undefined?{}:{inputJson:raw??JSON.stringify(value)},...symbol===undefined?{}:{symbol},result:guardCapture(()=>exportsObject[name](value,symbol))});
 }
}
for(const value of orderFrames)orderListCase(value);
for(const one of orderCases){if(!one.inputJson)continue;const raw='['+one.inputJson+']';orderListCase(JSON.parse(raw),raw);}
const orderStateCases=[];
for(const expected of [...values,'order-1','order-2'])for(const value of [undefined,null,orderBase,{...orderBase,order_id:'order-2'},{...orderBase,post_only:null}])orderStateCases.push({...inputJson(value),...arg('expectedJson',expected),result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseOrderState(value,expected))});
const tradeNames=['decodeCoinbaseStarbaseTrade','decodeCoinbaseStarbaseTradePage','normalizeCoinbaseStarbaseTrade'];
const tradeDefinitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>tradeNames.includes(d.name.getText(source))));
assert.equal(tradeDefinitions.length,tradeNames.length);
vm.runInNewContext(ts.transpileModule(tradeDefinitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+tradeNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...exportsObject,exports:exportsObject});
const tradeNormalizer=tradeDefinitions.flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='normalizeCoinbaseStarbaseTrade').initializer;
assert(ts.isTypeReferenceNode(tradeNormalizer.type));
const tradeNormalizerReturn={name:tradeNormalizer.type.typeName.getText(source),arguments:tradeNormalizer.type.typeArguments.map(type=>type.getText(source))};
const tradeBase={instrument_name:'BTC_USDC-PERPETUAL',matching_id:null,order_type:'limit',fee_currency:'USDC',trade_id:'trade-1',trade_seq:5,order_id:'order-1',timestamp:101,direction:'buy',state:'filled',price:7,mark_price:11,index_price:13,amount:17,fee:-19};
const tradeOptional={liquidity:'M',contracts:23,profit_loss:29,label:''};
const tradeCases=[];
function tradeCase(value,raw){
 const result=guardCapture(()=>exportsObject.decodeCoinbaseStarbaseTrade(value));let assessment;
 if(result.ok){const decoded=exportsObject.decodeCoinbaseStarbaseTrade(value);const assessed=exportsObject.normalizeCoinbaseStarbaseTrade(decoded);assert.strictEqual(assessed.gateway,decoded);assessment=preserveSpecialNumbers(assessed);}
 tradeCases.push({...value===undefined?{}:{inputJson:raw??JSON.stringify(value)},result,...assessment===undefined?{}:{assessment}});
}
for(const value of values)tradeCase(value);
tradeCase(tradeBase);
const tradeFields=Object.keys({...tradeBase,...tradeOptional});
const tradeValues=[...values,'BTC_USDC-PERPETUAL','ETH_USDC-PERPETUAL',...schemas.CoinbaseStarbaseTradeType.values,...schemas.CoinbaseStarbaseFeeCurrency.values,...schemas.CoinbaseStarbaseTradeState.values,...schemas.CoinbaseStarbaseOrderDirection.values,'M','T',Number.MAX_VALUE,Number.MIN_VALUE];
for(const field of tradeFields)for(const value of tradeValues){const input={...tradeBase,[field]:value};tradeCase(input,Object.is(value,-0)?JSON.stringify(input).replace('"'+field+'":0','"'+field+'":-0'):undefined);}
for(const a of tradeFields)for(const b of tradeFields)if(a!==b)tradeCase({...tradeBase,[a]:null,[b]:null});
for(let mask=0;mask<16;mask++){const input={...tradeBase};Object.entries(tradeOptional).forEach(([key,value],i)=>{if(mask&(1<<i))input[key]=value;});tradeCase(input);}
for(const state of schemas.CoinbaseStarbaseTradeState.values)for(const type of schemas.CoinbaseStarbaseTradeType.values)for(const fee of schemas.CoinbaseStarbaseFeeCurrency.values)for(const direction of schemas.CoinbaseStarbaseOrderDirection.values)for(const liquidity of [undefined,'M','T'])tradeCase({...tradeBase,state,order_type:type,fee_currency:fee,direction,liquidity});
tradeCase({...tradeBase,unknown:42});
const tradePageCases=[];
function tradePageCase(value,raw){tradePageCases.push({...value===undefined?{}:{inputJson:raw??JSON.stringify(value)},result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseTradePage(value))});}
for(const value of values)tradePageCase(value);
for(const has_more of values)for(const trades of [undefined,null,[],[tradeBase],[tradeBase,tradeBase],[tradeBase,tradeBase,{...tradeBase,matching_id:'not null'}],[{...tradeBase,trade_id:'trade-2'},tradeBase]])tradePageCase({has_more,trades});
for(const one of tradeCases){if(!one.inputJson)continue;for(const has_more of [false,true]){const raw='{"has_more":'+has_more+',"trades":['+one.inputJson+']}';tradePageCase(JSON.parse(raw),raw);}}
const coreTypes=ts.createSourceFile('types.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/types.ts'),'utf8'));
const coreEnumNames=['OrderStatus','OrderSide','OrderType','OrderTimeInForce','OrderStopTriggerType'];
const coreEnumDefinitions=coreTypes.statements.filter(n=>ts.isEnumDeclaration(n)&&coreEnumNames.includes(n.name.text));assert.equal(coreEnumDefinitions.length,coreEnumNames.length);
const coreEnums={};vm.runInNewContext(ts.transpileModule(coreEnumDefinitions.map(n=>n.getText(coreTypes)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:coreEnums});
const mappingDomains={mapOrderStatus:'CoinbaseStarbaseOrderState',mapOrderSide:'CoinbaseStarbaseOrderDirection',mapOrderType:'CoinbaseStarbaseOrderType',mapTimeInForce:'CoinbaseStarbaseTimeInForce',mapTrigger:'CoinbaseStarbaseTriggerType'};
const mappingNames=Object.keys(mappingDomains);
const mappingDefinitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>mappingNames.includes(d.name.getText(source))));assert.equal(mappingDefinitions.length,mappingNames.length);
vm.runInNewContext(ts.transpileModule(mappingDefinitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+mappingNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...coreEnums,exports:exportsObject});
const mappingCases=[];
for(const [name,domain] of Object.entries(mappingDomains))for(const value of schemas[domain].values)for(const postOnly of name==='mapTimeInForce'?[false,true]:[undefined])mappingCases.push({name,value,...postOnly===undefined?{}:{postOnly},result:guardCapture(()=>exportsObject[name](value,postOnly))});
// Source-only witness for the pending normalizer's shared Order.info representation seam.
const normalizeDefinition=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(source)==='normalizeCoinbaseStarbaseOrder'));assert.equal(normalizeDefinition.length,1);
vm.runInNewContext(ts.transpileModule(normalizeDefinition[0].getText(source),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{...coreEnums,...exportsObject,exports:exportsObject});
const referenceGateway=exportsObject.decodeCoinbaseStarbaseOrder({...orderBase,amount:3,filled_amount:0,reduce_only:false});
const referenceReady=exportsObject.normalizeCoinbaseStarbaseOrder(referenceGateway);assert.equal(referenceReady.status,'ready');assert.strictEqual(referenceReady.value.info,referenceGateway);
const snapshotPrice=referenceReady.value.price;referenceGateway.label='changed';referenceGateway.price=NaN;
assert.equal(referenceReady.value.info.label,'changed');assert(Number.isNaN(referenceReady.value.info.price));assert.equal(referenceReady.value.price,snapshotPrice);
const rawNumericIngress=[];
for(const raw of ['-0','0','-1e-9999','1e400','-1e400']){
 const value=JSON.parse(raw), bytes=Buffer.alloc(8);bytes.writeDoubleLE(value);
 rawNumericIngress.push({kind:'finite',raw,bits:bytes.readBigUInt64LE().toString(16),finite:Number.isFinite(value),result:guardCapture(()=>exportsObject.requireFinite(value,'raw ingress') )});
 const priceJson=JSON.stringify(orderBase).replace('"price":17','"price":'+raw),order=JSON.parse(priceJson);
 rawNumericIngress.push({kind:'order-price',raw,bits:bytes.readBigUInt64LE().toString(16),finite:Number.isFinite(value),inputJson:priceJson,result:guardCapture(()=>exportsObject.decodeCoinbaseStarbaseOrder(order))});
 rawNumericIngress.push({kind:'untagged-price',raw,bits:bytes.readBigUInt64LE().toString(16),finite:Number.isFinite(value),result:guardCapture(()=>exportsObject.parseOrderPrice(value))});
}
const stateExports = {};
vm.runInNewContext(ts.transpileModule(stateClass.getText(source) + '\nexport { CoinbaseStarbaseReadOnlyState };', {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText, {...exportsObject, exports:stateExports});
const readOnlyState = new stateExports.CoinbaseStarbaseReadOnlyState();
const initialState = JSON.parse(JSON.stringify(readOnlyState));
const stateCases = [];
function stateCase(op, input, symbol) {
 const result = guardCapture(() => {
  if (op === 'account') return readOnlyState.applyAccountSummary(input);
  if (op === 'positions') return readOnlyState.applyPositions(input);
  if (op === 'openOrders') return readOnlyState.applyOpenOrders(input);
  if (op === 'orderState') return readOnlyState.applyOrderState('order-1', input);
  if (op === 'orderHistory') return readOnlyState.applyOrderHistory(input);
  if (op === 'orderHistoryByInstrument') return readOnlyState.applyOrderHistoryByInstrument(symbol, input);
  if (op === 'tradePage') return readOnlyState.applyTradePage(input);
  throw Error('unclassified read-only state operation ' + op);
 });
 stateCases.push({op, input: input === undefined ? null : input, ...(symbol === undefined ? {} : {symbol}), result, state: JSON.parse(JSON.stringify(readOnlyState))});
}
stateCase('account', accountBase);
stateCase('account', {...accountBase, balance:null});
stateCase('positions', [positionBase]);
stateCase('positions', [positionBase, positionBase]);
stateCase('openOrders', [orderBase]);
stateCase('openOrders', [orderBase, orderBase]);
stateCase('orderState', orderBase);
stateCase('orderState', {...orderBase, order_id:'order-2'});
stateCase('orderHistory', [orderBase]);
stateCase('orderHistory', [orderBase, orderBase]);
stateCase('orderHistoryByInstrument', [orderBase], 'BTC-PERP-INTX');
stateCase('orderHistoryByInstrument', [{...orderBase, instrument_name:'ETH_USDC-PERPETUAL', order_id:'order-2'}], 'BTC-PERP-INTX');
stateCase('tradePage', {has_more:false, trades:[tradeBase]});
stateCase('tradePage', {has_more:false, trades:[tradeBase, tradeBase]});

process.stdout.write(JSON.stringify({...captureHeader(),mappingCases,tradeNormalizerReturn,tradeCases,tradePageCases,orderCases,orderListCases,orderStateCases,priceCases,sharedOrderIdentitySupplement,sharedAccountIdentitySupplement,sharedPositionIdentitySupplement,sharedTradeIdentitySupplement,positionAssessmentReturn,accountAssessmentReturn,positionCases,positionsCases,instrumentCases,uniqueCases,accountCases,builderCases,guardCases,schemas,recordSchemas,recordCases,requestSample,parityCases,rawNumericIngress,initialState,stateCases}));
