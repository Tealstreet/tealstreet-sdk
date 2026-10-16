// Field-identity supplements are post-audit evidence; original fixture bytes stay intact.
const identity = process.argv.includes('--field-identity');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const filename=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo/woo.v3.contracts.ts');
const source=fs.readFileSync(filename,'utf8');
const exportsObject={};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject});
const program=ts.createProgram([filename],{noLib:true,noResolve:true,target:ts.ScriptTarget.Latest});
const checker=program.getTypeChecker();
const ast=program.getSourceFile(filename);
const declarations=new Map(ast.statements.filter(ts.isTypeAliasDeclaration).map(s=>[s.name.text,s]));
function indexedSchema(t,typeChecker) {
  // Runtime values cannot detect a lost const assertion or a broad annotation.
  const resolved=typeChecker.getTypeFromTypeNode(t);
  const members=resolved.isUnion()?resolved.types:[resolved];
  assert.ok(members.every(member=>(member.flags & ts.TypeFlags.StringLiteral)!==0),
    'WOO indexed contract must remain a closed string-literal set');
  return {union:members.map(member=>({literal:member.value}))};
}
function schema(t) {
  if(ts.isTypeOperatorNode(t)) return schema(t.type);
  if(ts.isTypeLiteralNode(t)) return {fields:t.members.map(m=>({name:m.name.text,optional:!!m.questionToken,type:schema(m.type)}))};
  if(ts.isArrayTypeNode(t)) return {array:schema(t.elementType)};
  if(ts.isUnionTypeNode(t)) return {union:t.types.map(schema)};
  if(ts.isLiteralTypeNode(t)) return {literal:t.literal.text};
  if(ts.isIndexedAccessTypeNode(t)) return indexedSchema(t,checker);
  if(ts.isTypeReferenceNode(t)&&t.typeName.getText(ast)==='Readonly')return schema(t.typeArguments[0]);
  return t.getText(ast);
}
const shapes=Object.fromEntries([...declarations].map(([name,decl])=>[name,schema(decl.type)]));
// Both controls mutate the actual declaration; unchanged runtime tables cannot pass.
const table=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations)
  .find(d=>d.name.getText(ast)==='WOO_V3_ENDPOINTS');
assert.ok(ts.isAsExpression(table.initializer));
const mutations=[
  source.slice(0,table.initializer.getStart(ast))+table.initializer.expression.getText(ast)+source.slice(table.initializer.end),
  source.slice(0,table.name.end)+': Record<string, string>'+source.slice(table.name.end),
];
for(const mutated of mutations) {
  const options={noLib:true,noResolve:true,target:ts.ScriptTarget.Latest};
  const host=ts.createCompilerHost(options);
  host.getSourceFile=(file,version)=>file===filename?ts.createSourceFile(file,
    'type Record<K extends string, V> = { [P in K]: V };\n'+mutated,version,true):undefined;
  const control=ts.createProgram([filename],options,host);
  const declaration=control.getSourceFile(filename).statements.find(s=>ts.isTypeAliasDeclaration(s)&&s.name.text==='WooV3Endpoint');
  assert.throws(()=>indexedSchema(declaration.type,control.getTypeChecker()),
    /WOO indexed contract must remain a closed string-literal set/);
}

let fieldIdentity = 0;
function sample(s) {
  if(typeof s==='string') {
    if(s==='string')return identity ? String(++fieldIdentity)+'.50' : '12.50';
    if(s==='number')return identity ? ++fieldIdentity+0.25 : 123.25;
    if(s==='boolean')return true;
    if(s==='T')return ['generic-data'];
    assert.ok(shapes[s],s);return sample(shapes[s]);
  }
  if('literal' in s)return s.literal;
  if(s.union)return sample(s.union[0]);
  if(s.array)return [sample(s.array)];
  return Object.fromEntries(s.fields.filter(f=>!f.optional).map(f=>[f.name,sample(f.type)]));
}
const cases=[];
for(const [name,shape] of Object.entries(shapes)) {
 const input=sample(shape);
 cases.push({name,label:'valid',input,accepted:true});
 if(shape.union) {
  for(const arm of shape.union) cases.push({name,label:'literal arm '+JSON.stringify(arm),input:sample(arm),accepted:true});
  cases.push({name,label:'unknown literal',input:'not-a-declared-variant',accepted:false});
 }
 function visit(s,segments) {
  if(typeof s==='string') {if(shapes[s]) visit(shapes[s],segments);return;}
  if(s.array)return visit(s.array,[...segments,0]);
  if(!s.fields)return;
  for(const f of s.fields) {
   const variants=[['missing',undefined,!!f.optional],['null',null,false]];
   if(identity && f.type==='boolean')variants.push(['field identity false',false,true]);
   if(f.optional)variants.push(['present',sample(f.type),true]);
   if(f.type.union)for(const arm of f.type.union)variants.push(['union '+JSON.stringify(arm),sample(arm),true]);
   if(identity && typeof f.type==='string' && shapes[f.type]?.union)for(const arm of shapes[f.type].union)variants.push(['field identity referenced union '+JSON.stringify(arm),sample(arm),true]);
   for(const [label,value,accepted] of variants) {
    const mutated=structuredClone(input),parent=segments.reduce((v,k)=>v[k],mutated);
    if(value===undefined)delete parent[f.name];else parent[f.name]=value;
    cases.push({name,label:[...segments,f.name,label].join('.'),input:mutated,accepted});
   }
   if(!f.optional)visit(f.type,[...segments,f.name]);
  }
 }
 visit(shape,[]);
}
const orderedTables=Object.fromEntries(['WOO_V3_REST_BASE_URL','WOO_V3_ENDPOINTS','WOO_V3_MIGRATION_GATES','WOO_V3_WEBSOCKET_MIGRATION'].map(k=>[k,Object.entries(exportsObject[k])]));
orderedTables.WOO_V3_WS_URL=Object.entries(exportsObject.WOO_V3_WS_URL).map(([k,v])=>[k,Object.entries(v)]);
const literalSets={};
function collectLiterals(s,path) {
 if(s && typeof s==='object') {
  const arms=s.union||[s];
  if(arms.every(a=>typeof a.literal==='string'))literalSets[path]=arms.map(a=>a.literal);
  if(s.fields)for(const f of s.fields)collectLiterals(f.type,path+'.'+f.name);
 }
}
for(const [name,shape] of Object.entries(shapes))collectLiterals(shape,name);
const fixture={constants:exportsObject,orderedTables,cases,literalSets};
const bytes=JSON.stringify(fixture,null,2)+'\n';
const target=path.join(__dirname,'../tests/fixtures/contracts.json');
if(process.argv.includes('--schema')) process.stdout.write(JSON.stringify(shapes));
else if(identity) process.stdout.write(JSON.stringify({capture:{label:"field-identity supplement",platform:process.platform,architecture:process.arch,node:process.version},fixture:fixture}));
else if(process.argv.includes('--check'))assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO V3 contract drift');
else fs.writeFileSync(target,bytes);
if(!identity && !process.argv.includes('--schema'))console.log(`WOO inert V3 oracle: ${Object.keys(shapes).length} declarations, ${cases.length} cases, ${Object.keys(exportsObject).length} constants`);
