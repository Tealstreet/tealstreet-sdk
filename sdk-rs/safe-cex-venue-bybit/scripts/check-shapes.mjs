import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import ts from 'typescript';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const source=ts.createSourceFile('types.ts',readFileSync(root+'packages/safe-cex/src/exchanges/bybit/bybit.types.ts','utf8'),ts.ScriptTarget.Latest,true);
const rates=ts.createSourceFile('rates.ts',readFileSync(root+'packages/safe-cex/src/exchanges/bybit/bybit.rateLimits.ts','utf8'),ts.ScriptTarget.Latest,true);
const declarations=new Map([...source.statements,...rates.statements].filter(n=>ts.isInterfaceDeclaration(n)||ts.isTypeAliasDeclaration(n)||ts.isEnumDeclaration(n)).map(n=>[n.name.text,n]));
function fieldName(node) {
 if(!ts.isComputedPropertyName(node)) return node.text;
 assert.ok(ts.isPropertyAccessExpression(node.expression));
 const declaration=declarations.get(node.expression.expression.getText());
 return declaration.members.find(m=>m.name.text===node.expression.name.text).initializer.text;
}
const scalarPaths=(value,path=[]) => value && typeof value==='object'
 ? Object.entries(value).flatMap(([key,child])=>scalarPaths(child,[...path,key]))
 : value===null ? [] : [{path,kind:typeof value,value}];
function samples(node, identity=false) {
  if(ts.isEnumDeclaration(node)) return node.members.map(m=>{assert.ok(ts.isStringLiteral(m.initializer));return m.initializer.text;});
  if(ts.isTypeReferenceNode(node)) return samples(declarations.get(node.typeName.getText()),identity);
  if(ts.isTypeAliasDeclaration(node)) return samples(node.type,identity);
  if(ts.isUnionTypeNode(node)) return node.types.flatMap(type=>samples(type,identity));
  if(ts.isLiteralTypeNode(node)) { assert.ok(ts.isStringLiteral(node.literal)); return [node.literal.text]; }
  if(ts.isArrayTypeNode(node)) { const values=samples(node.elementType,identity); return identity?[[],...values.map(value=>[value]),values,[...values].reverse()]:[[],values]; }
  if(node.kind===ts.SyntaxKind.StringKeyword) return ['雪:wire',''];
  if(node.kind===ts.SyntaxKind.NumberKeyword) return [123.5,0,-1];
  if(node.kind===ts.SyntaxKind.BooleanKeyword) return [true,false];
  if(ts.isInterfaceDeclaration(node)||ts.isTypeLiteralNode(node)) {
    // Own members alone are not a complete inherited field inventory.
    assert.ok(!ts.isInterfaceDeclaration(node) || !node.heritageClauses?.length, 'Bybit shape census refuses interface heritage');
    const fields=node.members.map(field=>{assert.ok(ts.isPropertySignature(field)); return {name:fieldName(field.name), optional:!!field.questionToken, values:samples(field.type,identity)};});
    const full=Object.fromEntries(fields.map(f=>[f.name,f.values[0]]));
    const cases=[full,Object.fromEntries(fields.filter(f=>!f.optional).map(f=>[f.name,f.values[0]]))];
    const populated=Object.fromEntries(fields.map(f=>[f.name,f.values.reduce((best,value)=>scalarPaths(value).length>scalarPaths(best).length?value:best)]));
    for(const baseline of identity?[full,populated]:[full])
      for(const field of fields) for(const value of field.values) cases.push({...baseline,[field.name]:value});
    return cases;
  }
  throw Error(`unhandled source type ${node.getText(source)}`);
}
// Keep the negative control inside the mandatory shape/records entrypoint.
const inherited = ts.createSourceFile('inherited-control.ts', 'interface ExtraNumeric { hiddenNumeric: number } interface Probe extends ExtraNumeric { visible: number }', ts.ScriptTarget.Latest, true);
assert.throws(() => samples(inherited.statements[1]), /Bybit shape census refuses interface heritage/, 'inherited numeric field must not disappear from the shape census');
const originalCases=[];
for(const [name,node] of declarations) for(const value of samples(node)) originalCases.push({type:name,value});
const supplement=[];
for(const [name,node] of declarations) for(const value of samples(node,true))
 if(!originalCases.some(c=>c.type===name&&JSON.stringify(c.value)===JSON.stringify(value)) && !supplement.some(c=>c.type===name&&JSON.stringify(c.value)===JSON.stringify(value))) supplement.push({type:name,value});
const cases=[...originalCases,...supplement];
if(process.argv.includes("--original-cases")) { writeFileSync(1,JSON.stringify(originalCases)+"\n"); process.exit(0); }
// Inventory same-valued scalar pairs on each record, including nested fields.
// Compare the complete corpus, not only its first all-default sample: each
// independently variable field must have a case that distinguishes its identity.
const at=(value,path)=>path.reduce((parent,key)=>parent?.[key],value);
const identityAudit=[];
let repairedPairs=0;
let originalEqualPairs=0;
let originalBlindPairs=0;
for(const type of new Set(cases.map(c=>c.type))) {
 const records=cases.filter(c=>c.type===type).map(c=>c.value);
 const pairs=new Map();
 for(const record of records) {
  const fields=scalarPaths(record);
  for(let i=0;i<fields.length;i++)for(let j=i+1;j<fields.length;j++) {
   const a=fields[i],b=fields[j];
   if(a.kind!==b.kind || !Object.is(a.value,b.value))continue;
   const key=JSON.stringify([a.path,b.path]);pairs.set(key,[a,b]);
  }
 }
 for(const [a,b] of pairs.values()) {
  const witness=records.findIndex(record=>{
   const av=at(record,a.path),bv=at(record,b.path);
   return typeof av===a.kind && typeof bv===b.kind && !Object.is(av,bv);
  });
  const values=path=>new Set(records.map(record=>at(record,path)).filter(value=>typeof value===a.kind));
  const variable=values(a.path).size>1 || values(b.path).size>1;
  assert.ok(!variable || witness>=0, `field identity missing: ${type} ${a.path.join('.')} versus ${b.path.join('.')}`);
  const originalWitness=originalCases.filter(c=>c.type===type).some(({value})=>{
   const av=at(value,a.path),bv=at(value,b.path);
   return typeof av===a.kind && typeof bv===b.kind && !Object.is(av,bv);
  });
  const originalEqual=originalCases.filter(c=>c.type===type).some(({value})=>{
   const av=at(value,a.path),bv=at(value,b.path);
   return typeof av===a.kind && typeof bv===b.kind && Object.is(av,bv);
  });
  if(originalEqual)originalEqualPairs++;
  if(originalEqual&&!originalWitness&&variable)originalBlindPairs++;
  if(variable&&!originalWitness)repairedPairs++;
  identityAudit.push({type,left:a.path.join('.'),right:b.path.join('.'),kind:a.kind,witness,constant:!variable});
 }
}
if(process.argv.includes('--identity-audit')) {
 writeFileSync(1,JSON.stringify({records: new Set(cases.map(c=>c.type)).size,cases:cases.length,originalCases:originalCases.length,supplementCases:supplement.length,repairedPairs,originalEqualPairs,originalBlindPairs,pairs:identityAudit},null,2)+"\n");
 process.exit(0);
}
if(process.argv.includes('--records')) {
 const first=name=>cases.find(c=>c.type===name).value;
 console.log(JSON.stringify({
  BybitApiKeyInfo:first('BybitApiKeyInfo'),
  BybitApiKeyPermissions:first('BybitApiKeyInfo').permissions,
  BybitTransactionRecord:first('BybitTransactionRecord'),
  BybitTransactionHistoryOptions:first('BybitTransactionHistoryOptions'),
  BybitRateLimitRow:first('BybitRateLimitRow'),
  BybitRateLimitAccountRates:first('BybitRateLimitRow').futures,
  BybitResponseMeta:{isSuccess:true,hasExplicitStatus:false,retCode:0,retMsg:'ok',rawResponse:{}},
  BybitExecutionAmounts:{price:1,amount:2},
 }));
 process.exit(0);
}
assert.equal(cases.length,439,'SHAPES-CARDINALITY source total');
const run=spawnSync(process.env.CARGO??'cargo',['run','--quiet','--locked','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--example','shapes'],{cwd:root,encoding:'utf8',input:JSON.stringify(cases),maxBuffer:8*1024*1024});
assert.equal(run.status,0,[run.error?.message,run.stderr].filter(Boolean).join("\n"));
const actual=JSON.parse(run.stdout);
assert.equal(actual.length,cases.length,'SHAPES-CARDINALITY expected/native outputs');
for(let i=0;i<cases.length;i++) assert.deepEqual(actual[i],{ok:cases[i].value},`field identity ${cases[i].type} ${i<originalCases.length?"original":"supplement"} sample ${i}`);
console.log(`Bybit type shapes: ${cases.length} source-generated round trips, including each union member and optional omission.`);
