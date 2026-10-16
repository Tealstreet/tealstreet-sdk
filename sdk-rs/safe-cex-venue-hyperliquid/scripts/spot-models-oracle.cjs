// Independent actual-declaration resolver, strict TypeScript checker and native
// JSON.parse projections. It does not import the Rust model writer.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const sourcePath=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.types.ts');
const source=ts.createSourceFile(sourcePath,fs.readFileSync(sourcePath,'utf8'),ts.ScriptTarget.Latest,true);
require('./interface-heritage.cjs')(ts,source);
const ROOTS=['Cloid','OidOrCloid','Meta','ClearinghouseState','UserFills','OrderResponse','WsTrade','WsBook','WsLevel','WsOrder','WsFill','WsUserFunding','WsLiquidation','WsNonUserCancel','SpotClearinghouseState','FrontendOpenOrders','L2Book','CandleSnapshot','AssetCtx','MetaAndAssetCtxs','UserFundingDelta','UserFundingEntry','UserFunding','UserNonFundingLedgerDelta','UserNonFundingLedgerEntry','UserNonFundingLedgerUpdates','FundingHistoryEntry','FundingHistory','SpotToken','SpotMarket','SpotMeta','SpotAssetCtx','SpotMetaAndAssetCtxs','UserOpenOrder','UserOpenOrders','OrderRequest','OrderWire','CancelOrderRequest','CancelByCloidRequest','ModifyRequest','ModifyWire','Signature','Notification','Candle','HyperliquidCandle','WsUserFill','WsUserFills','WsUserFundings','WsUserNonFundingLedgerUpdate','WsUserNonFundingLedgerUpdates'];
assert.equal(ROOTS.length,50,'independent source root floor');
const declarations=new Map();
for(const statement of source.statements)if(ts.isInterfaceDeclaration(statement)||ts.isTypeAliasDeclaration(statement)) {
  const name=statement.name.text;
  if(declarations.has(name)) {
    const previous=declarations.get(name);assert(ts.isInterfaceDeclaration(statement)&&ts.isInterfaceDeclaration(previous));
    assert.equal(previous.getText(source),statement.getText(source),`merged source interface ${name}`);
  } else declarations.set(name,statement);
}
function definition(name) {const node=declarations.get(name);assert(node,`actual declared reference ${name}`);return ts.isInterfaceDeclaration(node)?{members:node.members}:node.type;}
function resolve(node) {return ts.isTypeReferenceNode(node)?definition(node.typeName.getText(source)):node;}
function members(node) {return node.members ?? (ts.isTypeLiteralNode(node)?node.members:null);}
function literal(node) {return ts.isLiteralTypeNode(node)&&ts.isStringLiteral(node.literal)?node.literal.text:null;}
const raw=text=>({raw:text});
function generate(node,mode,context) {
  if(ts.isTypeReferenceNode(node)&&node.typeName.getText(source)==='Array')return [generate(node.typeArguments[0],mode,context),generate(node.typeArguments[0],mode,context)];
  node=resolve(node);
  if(ts.isParenthesizedTypeNode(node))return generate(node.type,mode,context);
  const fields=members(node);
  if(fields)return Object.fromEntries(fields.filter(field=>!field.questionToken||mode!==1).map(field=>[field.name.text,generate(field.type,mode,context)]));
  if(ts.isArrayTypeNode(node))return [generate(node.elementType,mode,context),generate(node.elementType,mode,context)];
  if(ts.isTupleTypeNode(node))return node.elements.map(item=>generate(item,mode,context));
  const text=literal(node);if(text!==null)return text;
  if(ts.isUnionTypeNode(node)) {
    const literals=node.types.map(literal);
    if(literals.every(value=>value!==null))return literals[mode%literals.length];
    // OidOrCloid is the only non-literal union in this new root closure.
    assert.equal(node.getText(source),'number | Cloid');
    return generate(node.types[mode%2===0?1:0],mode,context);
  }
  const index=context.index++;
  if(node.kind===ts.SyntaxKind.StringKeyword)return [
    `value:${context.name}:${index}:\"\\/\b\f\n\r\t`, '',
    `\ud800${index}😀\udfff\ufeff`, `\0é雪😀\u2028\ufeff:${index}`,
  ][mode];
  if(node.kind===ts.SyntaxKind.NumberKeyword)return raw([
    String(17.5+index/4), '-0', ['1e400','-1e400','9007199254740993','4.9406564584124654e-324'][index%4],
    ['1e-9999','-1e-9999','-9007199254740993','1.2345678901234567'][index%4],
  ][mode]);
  if(node.kind===ts.SyntaxKind.BooleanKeyword)return mode!==1&&index%2===0;
  throw Error('unsupported actual type '+ts.SyntaxKind[node.kind]);
}
function render(value) {
  if(value&&Object.hasOwn(value,'raw'))return value.raw;
  if(Array.isArray(value))return '['+value.map(render).join(',')+']';
  if(value&&typeof value==='object')return '{'+Object.entries(value).map(([key,value])=>JSON.stringify(key)+':'+render(value)).join(',')+'}';
  return JSON.stringify(value);
}
const units=text=>Array.from({length:text.length},(_,index)=>text.charCodeAt(index));
const bits=value=>{const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
function native(node,value) {
  if(ts.isTypeReferenceNode(node)&&node.typeName.getText(source)==='Array')return value.map(item=>native(node.typeArguments[0],item));
  node=resolve(node);
  if(ts.isParenthesizedTypeNode(node))return native(node.type,value);
  const fields=members(node);
  if(fields)return Object.fromEntries(fields.map(field=>[field.name.text,!Object.hasOwn(value,field.name.text)?{absent:true}:native(field.type,value[field.name.text])]));
  if(ts.isArrayTypeNode(node))return value.map(item=>native(node.elementType,item));
  if(ts.isTupleTypeNode(node))return node.elements.map((item,index)=>native(item,value[index]));
  if(literal(node)!==null)return value;
  if(ts.isUnionTypeNode(node))return node.types.every(item=>literal(item)!==null)?value:typeof value==='number'?bits(value):units(value);
  if(node.kind===ts.SyntaxKind.StringKeyword)return units(value);
  if(node.kind===ts.SyntaxKind.NumberKeyword)return bits(value);
  if(node.kind===ts.SyntaxKind.BooleanKeyword)return value;
  throw Error('unsupported native projection');
}
const cases=[];
for(const name of ROOTS) {
  const node=definition(name);
  for(let mode=0;mode<4;mode++) {
    const input=render(generate(node,mode,{name,index:0}));
    cases.push({name,mode,input,expectedAccepted:true,native:native(node,JSON.parse(input))});
  }
  const shape=resolve(node),fields=members(shape);
  const positional=fields?'[0]':'{}';
  let wrong;
  if(fields) {
    const value=generate(node,0,{name,index:0});
    const first=fields.find(field=>!field.questionToken);
    assert(first,'required source field floor '+name);
    value[first.name.text]=resolve(first.type).kind===ts.SyntaxKind.BooleanKeyword?'wrong':false;
    wrong=render(value);
  } else wrong=ts.isArrayTypeNode(shape)?'[false]':name==='Cloid'?'17':'true';
  for(const input of ['null','false',positional,wrong])cases.push({name,input,expectedAccepted:false});
}
assert.equal(cases.length,400,'independent 50 roots x eight cases');
// JSON.parse applies duplicate-key replacement before typed native projection.
// Type-check its canonical parsed object, not a duplicate-key TS object literal.
const duplicateType='{"limit":{"tif":"bad"},"limit":{"tif":"Alo","\\ud800":0},"trigger":{"triggerPx":-0,"isMarket":false,"tpsl":"tp","\\udfff":1},"\\ud800":2}';
const request='{"coin":"X","is_buy":true,"sz":17.5,"limit_px":91.25,"order_type":'+duplicateType+',"reduce_only":false}';
const wire='{"a":17.5,"b":true,"p":"P","s":"S","r":false,"t":'+duplicateType+'}';
const witnesses=[['OrderRequest',request],['OrderWire',wire],['ModifyRequest','{"oid":42,"order":'+request+'}'],['ModifyWire','{"oid":42,"order":'+wire+'}']].map(([name,input])=>({name,input,mode:'nested duplicate/unknown key',expectedAccepted:true,native:native(definition(name),JSON.parse(input))}));
const frontendInput='{"coin":"X","isPositionTpsl":true,"isTrigger":true,"limitPx":"91.25","oid":42,"orderType":"limit","origSz":"19","reduceOnly":false,"side":"B","sz":"17.5","timestamp":7,"triggerCondition":"none","triggerPx":"0"}';
witnesses.push({name:'FrontendOpenOrders',input:frontendInput,mode:'distinct frontend boolean pair',expectedAccepted:true,native:native(definition('FrontendOpenOrders'),JSON.parse(frontendInput))});
assert.equal(witnesses.length,5,'independent four nested orders plus distinct frontend boolean witness');
const checkedCases=[...cases,...witnesses];
const probePath=path.join(path.dirname(sourcePath),'__spot_models_probe__.ts');
const probe="import * as Spot from './hyperliquidspot.types';\n"+checkedCases.map((row,index)=>index<400?`const case_${index}:Spot.${row.name}=${row.input};`:`const parsed_${index}=${JSON.stringify(JSON.parse(row.input))} as const; const case_${index}:Spot.${row.name}=parsed_${index};`).join('\n');
const options={strict:true,noEmit:true,skipLibCheck:true,target:ts.ScriptTarget.ES2022};
const host=ts.createCompilerHost(options),read=host.readFile.bind(host),exists=host.fileExists.bind(host);
host.readFile=file=>file===probePath?probe:read(file);host.fileExists=file=>file===probePath||exists(file);
const program=ts.createProgram([probePath],options,host),file=program.getSourceFile(probePath);assert(file);
assert.equal(program.getSyntacticDiagnostics(file).length,0,'actual typed probe syntax');
const rejected=new Set();
for(const diagnostic of program.getSemanticDiagnostics(file)) {
  assert(diagnostic.start!==undefined,'located actual checker diagnostic');
  const statement=file.statements.find(item=>item.getStart(file)<=diagnostic.start&&diagnostic.start<item.end);
  assert(statement&&ts.isVariableStatement(statement),'source diagnostic must belong to a named variable case: '+ts.flattenDiagnosticMessageText(diagnostic.messageText,' '));
  const match=/^case_(\d+)$/.exec(statement.declarationList.declarations[0].name.text);
  assert(match,'named source case');
  const caseIndex=Number(match[1]);
  assert([2322,2740,2741,2739,2559,2353].includes(diagnostic.code),`unexpected source diagnostic ${diagnostic.code}`);
  rejected.add(caseIndex);
}
for(const [index,row] of checkedCases.entries()) {
  row.accepted=!rejected.has(index);
  assert.equal(row.accepted,row.expectedAccepted,`strict actual spot checker ${row.name} case ${index}: ${row.input}`);
}
console.log(JSON.stringify({roots:ROOTS,cases,witnesses,iterationStop:399,scope:'50 new declared model roots; raw JSON string/slice ingress; declared fields, not excess properties/prototypes or live alias identity'}));
