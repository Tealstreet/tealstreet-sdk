const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const {indexedStringLiterals}=require('./literal-tuples.cjs');
const source=ts.createSourceFile('write-orders.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-orders.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const names=['WriteOrderType','LinkedOrderType','TriggerFillCondition','WriteSide','WriteParityBlocker'].map(name=>'CoinbaseStarbase'+name);
const tableNames=['STARBASE_WRITE_ORDER_TYPES','STARBASE_TRIGGER_FILL_CONDITIONS'];
const tables=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>tableNames.includes(d.name.getText(source))));assert.equal(tables.length,tableNames.length);
const exportsObject={};
vm.runInNewContext(ts.transpileModule(tables.map(n=>n.getText(source)).join('\n')+'\nexport {'+tableNames.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject});
function literals(node){
 if(ts.isIndexedAccessTypeNode(node)){
  const {name,values}=indexedStringLiterals(ts,node.getSourceFile(),node);if(node.getSourceFile()===source)assert.deepEqual(Array.from(exportsObject[name]),values,'executed write tuple matches inferred literals');return values;
 }
 if(ts.isUnionTypeNode(node))return node.types.flatMap(literals);
 if(ts.isLiteralTypeNode(node)&&ts.isStringLiteral(node.literal))return [node.literal.text];
 throw Error('unsupported write literal declaration '+node.getText(source));
}
const aliases=source.statements.filter(ts.isTypeAliasDeclaration);
const stringLiteral=node=>ts.isLiteralTypeNode(node)&&ts.isStringLiteral(node.literal);
const observed=aliases.filter(n=>ts.isIndexedAccessTypeNode(n.type)||stringLiteral(n.type)||(ts.isUnionTypeNode(n.type)&&n.type.types.every(stringLiteral))).map(n=>n.name.text);
assert.deepEqual(observed,names,'all top-level write literal unions classified');
const schemas=Object.fromEntries(names.map(name=>[name,{kind:'enum',values:literals(aliases.find(n=>n.name.text===name).type)}]));
const privateSource=ts.createSourceFile('private.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-private.ts'),'utf8'),ts.ScriptTarget.Latest,true);
function importedDeclaration(local,module,file,predicate){
 const bindings=source.statements.filter(ts.isImportDeclaration).flatMap(declaration=>{
  const imports=declaration.importClause?.namedBindings;
  return imports&&ts.isNamedImports(imports)?Array.from(imports.elements).filter(s=>s.name.text===local).map(specifier=>({declaration,specifier})):[];
 });
 assert.equal(bindings.length,1,'one import binding '+local);
 const {declaration,specifier}=bindings[0];
 assert(ts.isStringLiteral(declaration.moduleSpecifier));
 assert.equal(declaration.moduleSpecifier.text,module,'classified module '+local);
 const name=(specifier.propertyName??specifier.name).text;
 const found=file.statements.find(n=>predicate(n)&&n.name.text===name);assert(found,'resolved import '+local);return found;
}
const gatewayTif=importedDeclaration('CoinbaseStarbaseGatewayTimeInForce','./coinbase.starbase-private',privateSource,ts.isTypeAliasDeclaration);
const gatewayTifValues=indexedStringLiterals(ts,privateSource,gatewayTif.type).values;
const triggerAlias=importedDeclaration('CoinbaseStarbaseTriggerType','./coinbase.starbase-private',privateSource,ts.isTypeAliasDeclaration);
const triggerValues=indexedStringLiterals(ts,privateSource,triggerAlias.type).values;
const coreSource=ts.createSourceFile('types.ts',fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const coreTif=importedDeclaration('OrderTimeInForce','../../types',coreSource,ts.isEnumDeclaration);
const coreTifValues=coreTif.members.map(member=>{assert(member.initializer&&ts.isStringLiteral(member.initializer));return member.initializer.text;});
function shape(node){
 if(node.kind===ts.SyntaxKind.StringKeyword)return {kind:'string'};
 if(node.kind===ts.SyntaxKind.NumberKeyword)return {kind:'number'};
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return {kind:'boolean'};
 if(ts.isTypeOperatorNode(node)){assert.equal(node.operator,ts.SyntaxKind.ReadonlyKeyword);assert(ts.isTupleTypeNode(node.type));return shape(node.type);}
 if(ts.isArrayTypeNode(node))return {kind:'array',item:shape(node.elementType)};
 if(ts.isTupleTypeNode(node))return {kind:'tuple',items:node.elements.map(shape)};
 if(ts.isLiteralTypeNode(node)&&node.literal.kind===ts.SyntaxKind.TrueKeyword)return {kind:'enum',values:[true]};
 if(ts.isIndexedAccessTypeNode(node)&&ts.isTypeReferenceNode(node.objectType)){
  const object=shape(node.objectType);assert.equal(object.kind,'object');assert(ts.isLiteralTypeNode(node.indexType)&&ts.isStringLiteral(node.indexType.literal));
  const field=object.fields.find(f=>f.name===node.indexType.literal.text);assert(field,'indexed field exists');return field.schema;
 }
 if(ts.isTemplateLiteralTypeNode(node)){
  let values=[node.head.text];for(const span of node.templateSpans){const type=shape(span.type);assert.equal(type.kind,'enum');assert(type.values.every(v=>typeof v==='string'));values=values.flatMap(prefix=>type.values.map(value=>prefix+value+span.literal.text));}return {kind:'enum',values};
 }
 if(ts.isTypeReferenceNode(node)){
  const name=node.typeName.getText(node.getSourceFile());
  if(['Method','Params'].includes(name))return {kind:'parameter',name};
  if(name==='CoinbaseStarbasePrivateRequest'){
   const generic=importedDeclaration(name,'./coinbase.starbase-private',privateSource,ts.isTypeAliasDeclaration);
   assert.equal(generic.typeParameters.length,2);assert.equal(node.typeArguments.length,2);
   const [method,params]=generic.typeParameters;assert.equal(method.name.text,'Method');assert.equal(params.name.text,'Params');assert(!method.default&&!params.default);
   assert.equal(method.constraint.kind,ts.SyntaxKind.StringKeyword);
   assert(ts.isTypeReferenceNode(params.constraint));assert.equal(params.constraint.typeName.getText(privateSource),'Record');assert.equal(params.constraint.typeArguments.length,2);
   assert.equal(params.constraint.typeArguments[0].kind,ts.SyntaxKind.StringKeyword);assert.equal(params.constraint.typeArguments[1].kind,ts.SyntaxKind.UnknownKeyword);
   const replacements={Method:shape(node.typeArguments[0]),Params:shape(node.typeArguments[1])};
   function substitute(value){if(value&&value.kind==='parameter'){assert(replacements[value.name]);return replacements[value.name];}if(Array.isArray(value))return value.map(substitute);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,v])=>[key,substitute(v)]));return value;}
   return substitute(shape(generic.type));
  }
  if(node.getSourceFile()===privateSource){const alias=privateSource.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);assert(alias,'resolved nested private alias '+name);return shape(alias.type);}
  if(['CoinbaseStarbasePrivateOrder','CoinbaseStarbasePrivateTrade'].includes(name))return shape(importedDeclaration(name,'./coinbase.starbase-private',privateSource,ts.isTypeAliasDeclaration).type);
  if(name==='Omit'){
   assert.equal(node.typeArguments.length,2);const base=shape(node.typeArguments[0]);assert.equal(base.kind,'object');
   const keys=literals(node.typeArguments[1]);for(const key of keys)assert(base.fields.some(f=>f.name===key));
   return {...base,fields:base.fields.filter(f=>!keys.includes(f.name))};
  }
  if(['T','Intent'].includes(name))return {kind:'parameter',name};
  if(name==='OrderTimeInForce')return {kind:'enum',values:coreTifValues};
  if(name==='CoinbaseStarbaseTriggerType')return {kind:'enum',values:triggerValues};
  if(name==='CoinbaseStarbaseGatewayTimeInForce')return {kind:'enum',values:gatewayTifValues};
  const alias=aliases.find(n=>n.name.text===name);assert(alias,'known write type '+name);return shape(alias.type);
 }
 if(ts.isTypeQueryNode(node)){
  const name=node.exprName.getText(source);assert.equal(name,'COINBASE_STARBASE_BASE_AMOUNT_SOURCE');
  const declaration=source.statements.filter(ts.isVariableStatement).flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)===name);
  assert(!declaration.type&&ts.isAsExpression(declaration.initializer));assert.equal(declaration.initializer.type.getText(source),'const');assert(ts.isStringLiteral(declaration.initializer.expression));
  return {kind:'enum',values:[declaration.initializer.expression.text]};
 }
 if(ts.isTypeLiteralNode(node))return {kind:'object',fields:node.members.map(field=>({name:field.name.text,optional:!!field.questionToken,schema:shape(field.type)}))};
 if(ts.isUnionTypeNode(node)&&!node.types.every(stringLiteral))return {kind:'union',variants:node.types.map(shape)};
 return {kind:'enum',values:literals(node)};
}
const recordNames=['CoinbaseStarbaseBaseAmountIntent','CoinbaseStarbaseContractAmountIntent','CoinbaseStarbaseWriteAmountIntent','CoinbaseStarbaseWriteGate','CoinbaseStarbaseUnresolvedValidUntilIntent','CoinbaseStarbaseTifParams','CoinbaseStarbaseEditIntent','CoinbaseStarbaseEditParams','CoinbaseStarbaseClosePositionIntent','CoinbaseStarbaseOtocoLegIntent','CoinbaseStarbaseOtocoIntent','CoinbaseStarbaseOrderIdentity','CoinbaseStarbaseWriteResult','CoinbaseStarbasePlaceOrderIntent','CoinbaseStarbaseSideOrderIntent','CoinbaseStarbaseOtocoParams','CoinbaseStarbasePlaceOrderParams','CoinbaseStarbasePlaceOrderRequest','CoinbaseStarbaseEditRequest'];
const recordSchemas=Object.fromEntries(recordNames.map(name=>{
 const node=aliases.find(n=>n.name.text===name);assert(node,name);
 return [name,{...shape(node.type),...(node.typeParameters?{parameters:node.typeParameters.map(p=>({name:p.name.text,constraint:p.constraint?shape(p.constraint):null,default:p.default?shape(p.default):null}))}:{})}];
}));
for(const [anonymous,owner,field] of [
 ['CoinbaseStarbasePlaceOrderMethod','CoinbaseStarbasePlaceOrderRequest','method'],
 ['CoinbaseStarbaseEditMethod','CoinbaseStarbaseEditRequest','method'],
 ['CoinbaseStarbaseOtocoLegType','CoinbaseStarbaseOtocoLegIntent','type'],
 ['CoinbaseStarbaseClosePositionType','CoinbaseStarbaseClosePositionIntent','type'],
 ['CoinbaseStarbaseBaseAmountSource','CoinbaseStarbaseBaseAmountIntent','source'],
 ['CoinbaseStarbaseBaseAmountUnit','CoinbaseStarbaseBaseAmountIntent','unit'],
 ['CoinbaseStarbaseContractAmountSource','CoinbaseStarbaseContractAmountIntent','source'],
 ['CoinbaseStarbaseContractAmountUnit','CoinbaseStarbaseContractAmountIntent','unit'],
 ['CoinbaseStarbaseUnresolvedTimestampUnit','CoinbaseStarbaseUnresolvedValidUntilIntent','unit'],
])schemas[anonymous]=recordSchemas[owner].fields.find(f=>f.name===field).schema;
function sample(schema,choice=0,mask=0){
 switch(schema.kind){
  case 'string':return 'sample';
  case 'array':return [sample(schema.item),sample(schema.item)];
  case 'tuple':return schema.items.map(item=>sample(item));
  case 'number':return 12.25;
  case 'boolean':return false;
  case 'enum':return schema.values[0];
  case 'parameter':return {marker:schema.name};
  case 'object':{let bit=0;return Object.fromEntries(schema.fields.filter(f=>!f.optional||(mask&(1<<bit++))).map(f=>[f.name,sample(f.schema,choice)]));}
  case 'union':return sample(schema.variants[choice]);
  default:throw Error('unclassified write record sample '+schema.kind);
 }
}
const recordCases=recordNames.flatMap(name=>{
 const schema=recordSchemas[name];
 const choices=['CoinbaseStarbaseOrderIdentity','CoinbaseStarbaseWriteResult'].includes(name)?3:schema.kind==='union'?schema.variants.length:['CoinbaseStarbaseEditIntent','CoinbaseStarbaseClosePositionIntent','CoinbaseStarbasePlaceOrderIntent','CoinbaseStarbaseSideOrderIntent','CoinbaseStarbasePlaceOrderRequest'].includes(name)?2:1;
 const masks=2**(schema.fields??[]).filter(f=>f.optional).length;
 return Array.from({length:choices},(_,choice)=>Array.from({length:masks},(_,mask)=>{
  const value=sample(schema,name==='CoinbaseStarbaseWriteResult'?0:choice,mask);
  if(name==='CoinbaseStarbaseWriteResult'){value.trades=value.trades.slice(0,choice);value.trades.forEach((trade,index)=>trade.id='trade-'+index);if(choice===2)value.order.price='market_price';}
  if(name==='CoinbaseStarbasePlaceOrderRequest')value.method=schema.fields.find(f=>f.name==='method').schema.values[choice];
  if(name==='CoinbaseStarbaseOrderIdentity'){value.canonicalOrderId='canonical';value.nonCanonicalLabel='noncanonical';value.otocoChildOrderIds=[[],['first'],['second','first','second']][choice];}
  if(name==='CoinbaseStarbaseClosePositionIntent')value.type=schema.fields.find(f=>f.name==='type').schema.values[choice];
  return {name,choice,mask,value};
 })).flat();
});
const tifCases=gatewayTifValues.flatMap(value=>[false,true].map(post_only=>({post_only,time_in_force:value})));
const otocoCases=schemas.CoinbaseStarbaseWriteSide.values.flatMap(side=>coreTifValues.flatMap(tif=>triggerValues.flatMap(trigger=>schemas.CoinbaseStarbaseOtocoLegType.values.flatMap(type=>schemas.CoinbaseStarbaseTriggerFillCondition.values.map(condition=>{
 const first=sample(recordSchemas.CoinbaseStarbaseOtocoLegIntent,0,63),second=sample(recordSchemas.CoinbaseStarbaseOtocoLegIntent);
 Object.assign(first,{label:'first',direction:side,timeInForce:tif,trigger,type});second.label='second';
 return {side,tif,trigger,type,condition,value:{legs:[first,second],linkedOrderType:schemas.CoinbaseStarbaseLinkedOrderType.values[0],triggerFillCondition:condition}};
})))));
process.stdout.write(JSON.stringify({schemas,recordSchemas,recordCases,tifCases,otocoCases}));
