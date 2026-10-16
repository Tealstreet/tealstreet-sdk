const {ts, ast, values} = require('./source.cjs');
const assert = require('node:assert/strict');
const schemas = {};
const refs = {};
const fs = require('node:fs'), path = require('node:path');
const shared = ts.createSourceFile('types.ts',fs.readFileSync(path.join(require('./source.cjs').root,'packages/safe-cex/src/types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
for(const n of shared.statements)if(ts.isEnumDeclaration(n))refs[n.name.text]={kind:'union',items:n.members.map(m=>{assert(m.initializer && ts.isStringLiteral(m.initializer));return {kind:'literal',value:m.initializer.text};})};
function shape(n) {
 if(n.kind===ts.SyntaxKind.StringKeyword)return {kind:'string'};
 if(n.kind===ts.SyntaxKind.NumberKeyword)return {kind:'number'};
 if(n.kind===ts.SyntaxKind.BooleanKeyword)return {kind:'boolean'};
 if(ts.isLiteralTypeNode(n))return {kind:'literal',value:n.literal.text};
 if(ts.isIntersectionTypeNode(n)){const parts=n.types.map(shape);assert(parts.every(x=>x.kind==='object'));return {kind:'object',fields:parts.flatMap(x=>x.fields)};}
 if(ts.isArrayTypeNode(n))return {kind:'array',item:shape(n.elementType)};
 if(ts.isTupleTypeNode(n))return {kind:'tuple',items:n.elements.map(shape)};
 if(ts.isTypeReferenceNode(n)){
  if(n.typeName.getText()==='Array')return {kind:'array',item:shape(n.typeArguments[0])};
  assert(schemas[n.typeName.getText()]||refs[n.typeName.getText()], `Unresolved type ${n.getText()}`);
  return schemas[n.typeName.getText()]||refs[n.typeName.getText()];
 }
 if(ts.isUnionTypeNode(n))return {kind:'union',items:n.types.map(shape)};
 if(ts.isTypeLiteralNode(n))return record(n.members);
 throw Error(`Unrecognized type syntax: ${n.getText()}`);
}
function record(members){const index=members.find(ts.isIndexSignatureDeclaration);if(index){assert.equal(index.type.kind,ts.SyntaxKind.AnyKeyword);assert.equal(index.parameters[0].type.kind,ts.SyntaxKind.StringKeyword);}return {kind:'object',index:!!index,fields:members.filter(ts.isPropertySignature).map(m=>({name:m.name.getText(),optional:!!m.questionToken,shape:shape(m.type)}))};}
refs.PlaceOrderOpts=shape(shared.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='PlaceOrderOpts').type);
for(const n of ast.statements){
 if(ts.isInterfaceDeclaration(n))schemas[n.name.text]=record(n.members);
 if(ts.isTypeAliasDeclaration(n)&&n.name.text!=='AsterDEXStpMode')schemas[n.name.text]=shape(n.type);
 if(ts.isTypeAliasDeclaration(n)&&n.name.text==='AsterDEXStpMode') {
  // Fail closed if the indexed-access expression or its as-const source changes.
  assert.equal(n.type.getText(),'(typeof ASTERDEX_STP_MODES)[number]');
  const declaration=ast.statements.flatMap(s=>ts.isVariableStatement(s)?[...s.declarationList.declarations]:[]).find(d=>d.name.getText()==='ASTERDEX_STP_MODES');
  assert(ts.isAsExpression(declaration.initializer));
  assert.equal(declaration.initializer.type.getText(),'const');
  schemas[n.name.text]={kind:'union',items:values.ASTERDEX_STP_MODES.map(value=>({kind:'literal',value}))};
 }
}
function sample(s,path='wire',index=0){switch(s.kind){
 case 'string':return path;case 'number':return 1.25+index;case 'boolean':return index%2===0;case 'literal':return s.value;
 case 'union':return sample(s.items[0],path,index);case 'array':return [sample(s.item,`${path}[0]`,index)];case 'tuple':return s.items.map((x,i)=>sample(x,`${path}[${i}]`,i));
 case 'object':return Object.fromEntries(s.fields.map((f,i)=>[f.name,sample(f.shape,`${path}.${f.name}`,i)]));default:throw Error(s.kind);
}}
function accepts(s,v){switch(s.kind){
 case 'string':return typeof v==='string';case 'number':return typeof v==='number';case 'boolean':return typeof v==='boolean';case 'literal':return v===s.value;
 case 'union':return s.items.some(x=>accepts(x,v));case 'array':return Array.isArray(v)&&v.every(x=>accepts(s.item,x));case 'tuple':return Array.isArray(v)&&v.length===s.items.length&&s.items.every((x,i)=>accepts(x,v[i]));
 case 'object':return v!==null&&typeof v==='object'&&!Array.isArray(v)&&s.fields.every(f=>!(f.name in v)?f.optional:accepts(f.shape,v[f.name]));default:throw Error(s.kind);
}}
function witnesses(s){const base=sample(s), all=[base,null,[],{},true,false,42,'unknown'];
 if(s.kind==='object'){
  all.push(Object.values(base));
  if(s.index)for(const extra of [false,null,42,'payload',[1,'x'],{nested:[null,true]}])all.push({...base,unknownPayload:extra});
  for(const f of s.fields){const absent={...base};delete absent[f.name];all.push(absent);for(const v of witnesses(f.shape))all.push({...base,[f.name]:v});}
 } else if(s.kind==='literal'){all.push(s.value.toLowerCase(),s.value.toUpperCase(),' '+s.value,s.value+' ', '\ufeff'+s.value,{[s.value]:null},[s.value]);}
 else if(s.kind==='union')for(const x of s.items)all.push(...witnesses(x));
 else if(s.kind==='array')for(const x of witnesses(s.item))all.push([x]);
 else if(s.kind==='tuple'){all.push(base.slice(0,-1),[...base,'extra']);} 
 if(s.kind==='tuple')s.items.forEach((x,i)=>{for(const v of witnesses(x)){const a=[...base];a[i]=v;all.push(a);} });
 return all;
}
const tables=Object.fromEntries(Object.entries(values).filter(([,v])=>typeof v!=='function'));
const nestedSchemas={
 AsterDEXMarketResponseFiltersItem:schemas.AsterDEXMarketResponse.fields.find(f=>f.name==='filters').shape.item,
 AsterDEXAccountResponseAssetsItem:schemas.AsterDEXAccountResponse.fields.find(f=>f.name==='assets').shape.item,
 AsterDEXAccountResponsePositionsItem:schemas.AsterDEXAccountResponse.fields.find(f=>f.name==='positions').shape.item,
 AsterDEXAccountUpdateABItem:schemas.AsterDEXAccountUpdate.fields.find(f=>f.name==='a').shape.fields.find(f=>f.name==='B').shape.item,
 AsterDEXAccountUpdateAPItem:schemas.AsterDEXAccountUpdate.fields.find(f=>f.name==='a').shape.fields.find(f=>f.name==='P').shape.item,
 AsterDEXAccountUpdateA:schemas.AsterDEXAccountUpdate.fields.find(f=>f.name==='a').shape,
 AsterDEXOrderUpdateO:schemas.AsterDEXOrderUpdate.fields.find(f=>f.name==='o').shape,
 AsterDEXPositionModeResponseDualSidePositionString:{kind:'union',items:[{kind:'literal',value:'true'},{kind:'literal',value:'false'}]},
 AsterDEXPositionModeResponseDualSidePosition:{kind:'union',items:[{kind:'boolean'},{kind:'literal',value:'true'},{kind:'literal',value:'false'}]},
 AsterDEXStreamTickerE:schemas.AsterDEXStreamTicker.fields.find(f=>f.name==='e').shape,
 AsterDEXStreamDepthE:schemas.AsterDEXStreamDepth.fields.find(f=>f.name==='e').shape,
 AsterDEXStreamTradeE:schemas.AsterDEXStreamTrade.fields.find(f=>f.name==='e').shape,
 AsterDEXStreamMarkPriceE:schemas.AsterDEXStreamMarkPrice.fields.find(f=>f.name==='e').shape,
 AsterDEXAccountUpdateE:schemas.AsterDEXAccountUpdate.fields.find(f=>f.name==='e').shape,
 AsterDEXOrderUpdateE:schemas.AsterDEXOrderUpdate.fields.find(f=>f.name==='e').shape,
};
for(const [name,s] of Object.entries(nestedSchemas))schemas[name]=s;
const records=Object.fromEntries(Object.entries(schemas).map(([name,s])=>[name,[...new Map(witnesses(s).map(v=>[JSON.stringify(v),v])).values()].map(input=>({input,accept:accepts(s,input)}))]));
const numericRecords={};
function integerBaseline(s){let next=1;return JSON.parse(JSON.stringify(sample(s)), (_key,value)=>typeof value==='number'?next++:value);}
function numericRecord(name,s){
 if(s.kind==='object') {
  const fields=s.fields.filter(f=>f.shape.kind==='number').map(f=>f.name);
  if(fields.length)numericRecords[name]={sample:integerBaseline(s),fields};
  for(const f of s.fields){
   const child=name+f.name[0].toUpperCase()+f.name.slice(1);
   if(f.shape.kind==='object')numericRecord(child,f.shape);
   if(f.shape.kind==='array'&&f.shape.item.kind==='object')numericRecord(child+'Item',f.shape.item);
  }
 }else if(s.kind==='tuple')numericRecords[name]={sample:integerBaseline(s),fields:s.items.flatMap((x,i)=>x.kind==='number'?[i]:[])};
}
for(const [name,s] of Object.entries(schemas))numericRecord(name,s);
const numericBits=Object.fromEntries(['-0','-0.0','1','1.25','1e400','-1e400'].map(text=>{
 const value=JSON.parse(text);
 const b=Buffer.alloc(8);
 if(process.env.ASTERDEX_NUMERIC_CONTROL==='erase-zero'&&Object.is(value,-0))b.writeDoubleBE(0);
 else if(process.env.ASTERDEX_NUMERIC_CONTROL==='erase-finite'&&text==='1.25')b.writeDoubleBE(1);
 else b.writeDoubleBE(value);
 return [text,Number.isNaN(value)?'7ff8000000000000':b.toString('hex')];
}));
const recordFields=Object.fromEntries(Object.entries(schemas).filter(([,s])=>s.kind==='object').map(([name,s])=>[name,s.fields.map(field=>field.name)]));
let deepPayload=1;for(let i=0;i<140;i++)deepPayload=[deepPayload];
const deepRecords={};
for(const [name,record] of Object.entries(numericRecords)){
 if(schemas[name]?.kind!=='object')continue;
 const text=JSON.stringify({...record.sample,depthPayload:deepPayload});
 const parsed=JSON.parse(text);let leaf=parsed.depthPayload,depth=0;
 while(Array.isArray(leaf)){if(leaf.length!==1)throw Error('depth fixture cardinality');depth++;leaf=leaf[0];}
 if(depth!==140||leaf!==1||!accepts(schemas[name],parsed))throw Error('source depth fixture rejected '+name);
 deepRecords[name]={text,depth,leaf,accepted:true};
}
if(require.main===module)process.stdout.write(JSON.stringify({captureArchitecture:`${process.platform}-${process.arch}`,captureNodeMajor:Number(process.versions.node.split('.')[0]),tables,records,recordFields,numericRecords,numericBits,deepRecords}));
module.exports={schemas,shape,record,sample,witnesses,accepts};
