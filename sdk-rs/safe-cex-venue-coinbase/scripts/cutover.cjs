const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {indexedStringLiterals}=require('./literal-tuples.cjs');
const {cases}=require('./wire-cases.cjs');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-cutover.ts');
const text=fs.readFileSync(file,'utf8');
const source=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
const aliases=new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(n=>[n.name.text,n]));
const arrays={};
for(const statement of source.statements)if(ts.isVariableStatement(statement))for(const d of statement.declarationList.declarations){
 let n=d.initializer;if(n&&ts.isAsExpression(n))n=n.expression;
 if(n&&ts.isArrayLiteralExpression(n)){
  const exports={};vm.runInNewContext(ts.transpileModule('export const value = '+n.getText(source),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
  arrays[d.name.text]=exports.value;
 }
}
function schema(node){
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return {kind:'boolean'};
 if(ts.isTypeReferenceNode(node)){
  if(node.typeName.getText(source)==='Array')return {kind:'array',item:schema(node.typeArguments[0])};
  return schema(aliases.get(node.typeName.getText(source)).type);
 }
 if(ts.isArrayTypeNode(node))return {kind:'array',item:schema(node.elementType)};
 if(ts.isIndexedAccessTypeNode(node))return {kind:'enum',values:indexedStringLiterals(ts,source,node).values};
 if(ts.isLiteralTypeNode(node))return {kind:'enum',values:[node.literal.kind===ts.SyntaxKind.FalseKeyword?false:node.literal.text]};
 if(ts.isUnionTypeNode(node))return {kind:'enum',values:node.types.flatMap(n=>{const s=schema(n);assert.equal(s.kind,'enum');return s.values;})};
 assert.ok(ts.isTypeLiteralNode(node),'unsupported cutover declaration '+node.getText(source));
 return {kind:'object',fields:node.members.map(m=>({name:m.name.text,optional:!!m.questionToken,schema:schema(m.type)}))};
}
const schemas=Object.fromEntries([...aliases].filter(([,n])=>n.modifiers?.some(m=>m.kind===ts.SyntaxKind.ExportKeyword)).map(([name,n])=>[name,schema(n.type)]));
const protocol={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(path.dirname(file),'coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:protocol});
const runtime={};
vm.runInNewContext(ts.transpileModule(text+'\nexport {collectHttpShadowBlockers,collectWebsocketBlockers,validateEvidence};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:runtime,require:name=>{assert.equal(name,'./coinbase.starbase');return protocol;}});
let evidenceCases=[{}];
for(const field of schemas.CoinbaseStarbaseCutoverEvidence.fields){
 assert.equal(field.schema.kind,'enum');
 evidenceCases=evidenceCases.flatMap(previous=>field.schema.values.map(value=>({...previous,[field.name]:value})));
}
const collectors=evidenceCases.map(evidence=>({evidence,websocket:runtime.collectWebsocketBlockers(evidence),http:arrays.TEST_PRODUCT_CLASSIFICATIONS.map(product=>({product,blockers:runtime.collectHttpShadowBlockers(product,evidence)}))}));
const validation=[];
function probe(name,inputJson){
 let result;try{runtime.validateEvidence(inputJson===undefined?undefined:JSON.parse(inputJson));result={ok:true};}
 catch(error){result={error:{name:error.name,message:error.message}};}
 validation.push({name,inputJson,result});
}
probe('undefined',undefined);
for(const value of [null,false,true,0,1,'', 'x',[],[{}],...evidenceCases])probe('shape '+JSON.stringify(value),JSON.stringify(value));
const baseline=evidenceCases[0];
for(const key of Object.keys(baseline)){
 const missing={...baseline};delete missing[key];probe('missing '+key,JSON.stringify(missing));
 for(const value of [null,false,0,[],{},'', 'implemented','not-implemented','verified','unverified','constructor','\uFEFFverified'])probe('status '+key+' '+JSON.stringify(value),JSON.stringify({...baseline,[key]:value}));
}
const unknowns=['','z','a','2','1','01','-0','4294967294','4294967295','constructor','__proto__','toString','\uFEFF'];
for(const first of unknowns)for(const second of unknowns)if(first!==second){
 // Hand-built JSON preserves insertion order on the Rust side; JSON.stringify
 // would pre-sort numeric keys and conceal a missing js_own_keys call.
 const prefix=JSON.stringify(first)+':null,'+JSON.stringify(second)+':null';
 for(const rest of [JSON.stringify(baseline).slice(1,-1),''])probe('keys '+first+'/'+second+(rest?' complete':' missing'),'{'+prefix+(rest?','+rest:'')+'}');
}
const readiness=[];
for(const fixture of validation)for(const product of [undefined,...arrays.TEST_PRODUCT_CLASSIFICATIONS,null,false,0,'INTX','constructor',{},[]]){
 const options={evidence:fixture.inputJson===undefined?undefined:JSON.parse(fixture.inputJson),productClassification:product};
 let result;try{result={ok:runtime.evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly(options)}}catch(error){result={error:{name:error.name,message:error.message}}}
 readiness.push({inputJson:fixture.inputJson,...product===undefined?{}:{product},result});
}
const selector=source.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).find(d=>d.name.text==='selectCoinbaseStarbaseCutoverPlan');
assert.equal(selector.initializer.parameters.length,0,'production selector accepts no injected options');
const selectedPlan=runtime.selectCoinbaseStarbaseCutoverPlan();
process.stdout.write(JSON.stringify({selectedPlan,readiness,schemas,collectors,validation,types:Object.fromEntries(Object.entries(schemas).map(([name,s])=>[name,cases(s,name)]))}));
