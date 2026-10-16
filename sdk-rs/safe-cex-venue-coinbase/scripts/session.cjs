const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {indexedStringLiterals}=require('./literal-tuples.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
function load(file, imports={},extra='') {
 const exports={};
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8')+extra,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:name=>{
  assert.ok(Object.hasOwn(imports,name),`unexpected session dependency ${name}`);return imports[name];
 }});return exports;
}
const protocol=load('coinbase.starbase.ts');
const session=load('coinbase.starbase-session.ts',{'./coinbase.starbase':protocol},'\nexport { STARBASE_CANCEL_ON_DISCONNECT_SCOPES };');
const values=[undefined,null,false,true,0,1,'','connection','account','ok','ACCOUNT',[],{},{connection:null},{account:null},...session.STARBASE_CANCEL_ON_DISCONNECT_SCOPES];
const methods=['buildCoinbaseStarbaseEnableCancelOnDisconnect','buildCoinbaseStarbaseDisableCancelOnDisconnect','buildCoinbaseStarbaseGetCancelOnDisconnect'];
const record=value=>value===undefined?{}:{input:value};
function capture(fn,value){try{return {ok:fn(value)}}catch(e){return {error:{name:e.name,message:e.message}}}}
const builders=methods.flatMap(method=>values.map(value=>({method,...record(value),result:capture(session[method],value)})));
const mutations=values.map(value=>({...record(value),result:capture(session.decodeCoinbaseStarbaseCancelOnDisconnectMutationResult,value)}));
const getters=[];
// Decoder inputs cross JSON-RPC: do not compare pre-JSON own undefined keys
// against a Rust payload from which JSON.stringify has already erased them.
const add=original=>{
 const value=original===undefined?undefined:JSON.parse(JSON.stringify(original));
 getters.push({...record(value),result:capture(session.decodeCoinbaseStarbaseGetCancelOnDisconnectResult,value)});
};
values.forEach(add);
for(const enabled of values)for(const scope of values) {
 add({enabled,scope});add({scope,enabled});add({enabled,scope,extra:true});add({enabled,scope,zzz:true});
}
add(JSON.parse('{"enabled":true,"scope":"account","__proto__":{}}'));
const errors=['','test','\uFEFF','line\nline'].map(message=>{const e=new protocol.CoinbaseStarbaseProtocolError(message);return {name:e.name,message:e.message,display:e.toString()};});
const source=ts.createSourceFile('session.ts',fs.readFileSync(path.join(root,'coinbase.starbase-session.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const aliases=new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(n=>[n.name.text,n]));
function shape(node){
 if(node.kind===ts.SyntaxKind.StringKeyword)return {kind:'string'};
 if(node.kind===ts.SyntaxKind.UnknownKeyword)return {kind:'unknown'};
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return {kind:'boolean'};
 if(ts.isTypeLiteralNode(node))return {kind:'object',fields:node.members.map(field=>({name:field.name.text,optional:!!field.questionToken,schema:shape(field.type)}))};
 if(ts.isTypeReferenceNode(node)){
  const name=node.typeName.getText(source);
  if(name==='Record'){assert.equal(node.typeArguments.length,2);return {kind:'record',key:shape(node.typeArguments[0]),value:shape(node.typeArguments[1])};}
  if(['Method','Params'].includes(name))return {kind:'parameter',name};
  assert.ok(aliases.has(name),`unknown session type ${name}`);return shape(aliases.get(name).type);
 }
 if(ts.isIndexedAccessTypeNode(node)){
  const {name,values}=indexedStringLiterals(ts,source,node);
  assert.deepEqual(Array.from(session[name]),values,'session tuple values');
  return {kind:'enum',values};
 }

 throw Error(`unsupported session type ${node.getText(source)}`);
}
const schemas=Object.fromEntries([...aliases].map(([name,node])=>[name,{...shape(node.type),...(node.typeParameters?{parameters:node.typeParameters.map(parameter=>({name:parameter.name.text,constraint:shape(parameter.constraint),default:parameter.default?shape(parameter.default):null}))}:{})}]));
process.stdout.write(JSON.stringify({schemas,scopes:session.STARBASE_CANCEL_ON_DISCONNECT_SCOPES,builders,mutations,getters,errors}));
