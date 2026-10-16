const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase.ts');
const text=fs.readFileSync(file,'utf8'), exportsObject={};
vm.runInNewContext(ts.transpileModule(text+'\nexport {isRecord,isSafeRequestId,parseRpcError,validateAuthResult};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject});
const values=[undefined,null,false,true,0,-0,-1,1,1.5,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,1e30,1e-30,'', '1','0x10',[],{}, {code:1,message:'x'}];
const input=value=>value===undefined?{}:{input:value};
function capture(f,value){try{return {ok:f(value)}}catch(e){return {error:{name:e.name,message:e.message}}}}
const methods=[...values,'public/a','private/a','public/a_0','public/','private/0abc','PUBLIC/abc','public/a/b'];
for(const char of [...Array.from({length:128},(_,i)=>String.fromCharCode(i)),'\uFEFF','\u0085','\u2028','\u2029','é','😀'])for(const prefix of ['public/','private/','']){
 methods.push(prefix+char+'abc',prefix+'abc'+char);
}
const errors=[...values];
for(const code of values)for(const message of values){
 errors.push({code,message},{code,message,data:null},{code,message,data:{z:1,a:2}},{code,message,extra:'preserved?'});
}
for(const data of values)errors.push({code:-32700,message:"parse",data});
const canonical=original=>original===undefined?undefined:JSON.parse(JSON.stringify(original));
const source=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
const declaration=source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='CoinbaseStarbaseJsonRpcErrorObject');
function shape(node){
 if(ts.isTypeReferenceNode(node))return shape(source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===node.typeName.text).type);
 if(ts.isUnionTypeNode(node))return {kind:'enum',values:node.types.flatMap(n=>shape(n).values)};
 if(ts.isTypeOperatorNode(node)&&node.operator===ts.SyntaxKind.KeyOfKeyword){
  assert.ok(ts.isTypeQueryNode(node.type));
  return {kind:'enum',values:Object.keys(exportsObject[node.type.exprName.text])};
 }

 if(ts.isLiteralTypeNode(node)&&ts.isStringLiteral(node.literal))return {kind:'enum',values:[node.literal.text]};
 if(node.kind===ts.SyntaxKind.NumberKeyword)return {kind:'number'};
 if(node.kind===ts.SyntaxKind.StringKeyword)return {kind:'string'};
 if(node.kind===ts.SyntaxKind.UnknownKeyword)return {kind:'unknown'};
 assert.ok(ts.isTypeLiteralNode(node),'unsupported error object schema');
 return {kind:'object',fields:node.members.map(f=>({name:f.name.text,optional:!!f.questionToken,schema:shape(f.type)}))};
}
const authDeclaration=source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='CoinbaseStarbaseAuthResult');
const fixturePath=path.resolve(__dirname,'../../../packages/safe-cex/src/__tests__/audit-regressions/coinbase.starbase.fixtures.ts');
const fixtureExports={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(fixturePath,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:fixtureExports});
const errorIdentityInput={...fixtureExports.STARBASE_ERROR_RESPONSE.error,message:'fixture-message-identity',data:{marker:'fixture-data-identity'}};
const errorIdentitySupplement={label:'field-identity-supplement',input:errorIdentityInput,result:capture(exportsObject.parseRpcError,errorIdentityInput)};
const authBase={access_token:'token',expires_in:60,scope:'',token_type:'bearer'};
const auth=[...values,authBase,Object.values(authBase),{...authBase,extra:null}];
for(const key of Object.keys(authBase)){
 const missing={...authBase};delete missing[key];auth.push(missing);
 for(const value of [...values,'bearer','BEARER',' token ','\uFEFF','\u0085',{'bearer':null}])auth.push({...authBase,[key]:value});
}
const authCases=auth.map(canonical).map(v=>({...input(v),result:capture(value=>{exportsObject.validateAuthResult(value);return true},v)}));
const declarationNames=['CoinbaseStarbaseTransportKind','CoinbaseStarbaseTransportContext','CoinbaseStarbaseHeartbeatSignal','CoinbaseStarbaseTestResult'];
const declarations=Object.fromEntries(declarationNames.map(name=>[name,shape(source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name).type)]));
process.stdout.write(JSON.stringify({endpoints:Object.entries(exportsObject.COINBASE_STARBASE_ENDPOINTS).map(([kind,url])=>({kind,url,resolved:exportsObject.resolveCoinbaseStarbaseEndpoint(kind)})),declarations,schema:shape(declaration.type),authSchema:shape(authDeclaration.type),auth:authCases,errorIdentitySupplement,
 methods:methods.map(v=>({...input(v),result:capture(exportsObject.requireCoinbaseStarbaseMethod,v)})),
 records:values.map(v=>({...input(v),result:exportsObject.isRecord(v)})),
 ids:values.map(v=>({...input(v),result:exportsObject.isSafeRequestId(v)})),
 errors:errors.map(canonical).map(v=>({...input(v),result:exportsObject.parseRpcError(v)??null}))}));
