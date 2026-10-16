const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
const compilerOptions={module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022};
const contracts={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'woo.v3.contracts.ts'),'utf8'),{compilerOptions}).outputText,{exports:contracts});
const filename=path.join(root,'woo.v3.runtime-selector.ts'),source=fs.readFileSync(filename,'utf8'),ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const declaration=name=>ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);
const observed=declaration('WooV3ObservedCanonical').type;assert.equal(observed.typeName.getText(ast),'Readonly');assert.deepEqual(observed.typeArguments[0].members.map(m=>[m.name.getText(ast),m.type.getText(ast),!!m.questionToken]),[['canonical','string',false],['observed','TObserved',false]]);
const arms=declaration('WooV3ShadowReadComparison').type.types;assert.equal(arms.length,2);
for(const [i,fields]of [[0,['status','routeId','authoritative','observedCanonical','observed','storeAction']],[1,['status','routeId','authoritative','error','storeAction']]]){
 assert.equal(arms[i].typeName.getText(ast),'Readonly');const members=arms[i].typeArguments[0].members;assert.deepEqual(members.map(m=>m.name.getText(ast)),fields);assert(members.every(m=>!m.questionToken));
 const status=members[0].type;assert.deepEqual((status.types||[status]).map(t=>t.literal.text),i===0?['match','mismatch']:['decode-or-map-failed']);
 assert.equal(members.find(m=>m.name.text==='routeId').type.getText(ast),'WooV3MigrationRouteId');assert.equal(members.find(m=>m.name.text==='authoritative').type.getText(ast),'TLegacy');assert.equal(members.find(m=>m.name.text==='storeAction').type.literal.text,'retain-legacy');
 if(i===0){assert.equal(members.find(m=>m.name.text==='observedCanonical').type.kind,ts.SyntaxKind.StringKeyword);assert.equal(members.find(m=>m.name.text==='observed').type.getText(ast),'TObserved');}else{assert.equal(members.find(m=>m.name.text==='error').type.getText(ast),'Error');}
}
const program=ts.createProgram([filename],{target:ts.ScriptTarget.ES2022,strictNullChecks:true,noEmit:true,skipLibCheck:true}),checker=program.getTypeChecker(),checked=program.getSourceFile(filename);
const comparison=checked.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='WooV3ShadowReadComparison');
const errorMember=comparison.type.types[1].typeArguments[0].members.find(m=>m.name.text==='error'),errorType=checker.getTypeAtLocation(errorMember.type);
for(const field of ['name','message']) {const property=errorType.getProperty(field);assert(property);assert(!(property.flags&ts.SymbolFlags.Optional));assert.equal(checker.typeToString(checker.getTypeOfSymbolAtLocation(property,errorMember)),'string');}
const stack=errorType.getProperty('stack');assert(stack.flags&ts.SymbolFlags.Optional);assert.equal(checker.typeToString(checker.getTypeOfSymbolAtLocation(stack,errorMember)),'string | undefined');
const cause=errorType.getProperty('cause');assert(cause.flags&ts.SymbolFlags.Optional);assert.equal(checker.typeToString(checker.getTypeOfSymbolAtLocation(cause,errorMember)),'unknown');
const api={};vm.runInNewContext(ts.transpileModule(source+'\nexport {normalizeError}; export const nativeProofError=(message:string)=>new Error(message);',{compilerOptions}).outputText,{exports:api,require:name=>{assert.equal(name,'./woo.v3.contracts');return contracts;}});
function units(s){return Array.from({length:s.length},(_,i)=>s.charCodeAt(i));}
const modes=JSON.parse(fs.readFileSync(0,'utf8')),result=[];
for(const mode of modes){
 if(mode==='normalize-error'||mode==='normalize-bool'){
  const input=mode==='normalize-error'?api.nativeProofError(String.fromCharCode(0xd800,65)):false;const value=api.normalizeError(input);result.push({identity:value===input,units:units(value.message)});continue;
 }
 const legacy={legacy:17},observed={v3:19},response={response:23},original=api.nativeProofError('original error');let calls=0;
 const canonical=mode==='surrogate'?String.fromCharCode(0xd800,65):'same';
 const input={selection:{routeId:'markets'},legacySnapshot:legacy,legacyCanonical:canonical,v3Response:response,decodeAndCanonicalizeV3:function(value){
  assert.equal(this,input,'source decoder receiver');assert.equal(value,response,'source original response');assert.equal(arguments.length,1);calls++;
  switch(mode){
   case 'null':return null;case 'missing-canonical':return {observed};case 'missing-observed':return {canonical};case 'number-canonical':return {canonical:29,observed};case 'function':return ()=>undefined;
   case 'throw-error':throw original;case 'throw-bool':throw false;case 'mutate-throw':input.selection.routeId='tickers';throw original;
   case 'match':case 'mismatch':case 'undefined-observed':case 'surrogate':return {canonical:mode==='mismatch'?'different':canonical,observed:mode==='undefined-observed'?undefined:observed};
   default:throw Error('unknown fixture mode '+mode);
  }
 }};
 const value=api.compareWooV3ShadowRead(input);assert.equal(calls,1,'source decoder call count');assert.equal(value.authoritative,legacy,'source authoritative identity');assert(Object.isFrozen(value),'source frozen result');
 const success=value.status==='match'||value.status==='mismatch';
 if(success){assert.equal(value.observed,mode==='undefined-observed'?undefined:observed,'source observed identity');}else if(mode==='throw-error'||mode==='mutate-throw'){assert.equal(value.error,original,'source original Error');}
 result.push({keys:Object.keys(value),status:value.status,routeId:value.routeId,authoritativeIdentity:value.authoritative===legacy,observedIdentity:value.observed===observed,canonicalUnits:success?units(value.observedCanonical):null,errorUnits:success?null:units(value.error.message),frozen:Object.isFrozen(value),calls});
}
process.stdout.write(JSON.stringify(result));
