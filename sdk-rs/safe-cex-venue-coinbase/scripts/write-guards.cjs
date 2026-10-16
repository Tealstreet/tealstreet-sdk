const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
const source=ts.createSourceFile('write.ts',fs.readFileSync(path.join(root,'coinbase.starbase-orders.ts'),'utf8'));
const names=['protocolError','requireRecord','requireArray','requireString','parseEnum','assertUnique','decodeStringArray','validateLabel'];
const definitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(source))));assert.equal(definitions.length,names.length);
const imports=source.statements.filter(ts.isImportDeclaration).flatMap(declaration=>{
 const bindings=declaration.importClause?.namedBindings;return bindings&&ts.isNamedImports(bindings)?Array.from(bindings.elements).filter(s=>s.name.text==='CoinbaseStarbaseProtocolError').map(specifier=>({declaration,specifier})):[];
});assert.equal(imports.length,1);assert.equal(imports[0].declaration.moduleSpecifier.text,'./coinbase.starbase');assert.equal((imports[0].specifier.propertyName??imports[0].specifier.name).text,'CoinbaseStarbaseProtocolError');
const protocol={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:protocol});
const impl={};vm.runInNewContext(ts.transpileModule(definitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+names.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:impl,CoinbaseStarbaseProtocolError:protocol.CoinbaseStarbaseProtocolError});
function capture(fn){try{const value=fn();return value===undefined?{undefined:true}:{ok:value};}catch(error){return {error:{name:error.name,message:error.message}};}}
const values=[undefined,null,true,false,0,-0,1,-1,1.5,'',' ','a','b','buy','sell','constructor','__proto__','toString','\uFEFF','\u0085','💰',[],[1],{}, {z:1,a:2}, {toString:null}];
const contexts=['field','','\uFEFF','\u0085','💰','line\nbreak'];
const cases=[];
function add(name,input,context='',allowed){cases.push({name,...input===undefined?{}:{inputJson:Object.is(input,-0)?'-0':JSON.stringify(input)},context,...allowed?{allowed}:{},result:capture(()=>{
 const result=name==='parseEnum'?impl[name](input,allowed,context):impl[name](input,context);
 if(name==='requireRecord'||name==='requireArray')assert.strictEqual(result,input,'guard preserves reference');return result;
})});}
for(const input of values)for(const context of contexts)for(const name of ['requireRecord','requireArray','requireString','decodeStringArray','validateLabel'])add(name,input,context);
for(const allowed of [[],[''],['buy','sell'],['a','a'],['constructor','__proto__','toString'],['💰','\uFEFF','\u0085']])for(const input of [...values,...allowed])for(const context of contexts)add('parseEnum',input,context,allowed);
const pool=['a','b','',null,1,'💰','\uFEFF'];
for(const a of pool)for(const b of pool)for(const c of pool)for(const context of contexts){const input=[a,b,c];add('decodeStringArray',input,context);if(input.every(v=>typeof v==='string'))add('assertUnique',input,context);}
for(const context of contexts)for(const input of [[],['a'],['a','a'],['b','a','b']])add('assertUnique',input,context);
for(const unit of ['a','é','💰','e\u0301','\uFEFF','\u0085','\uD7FF','\uE000'])for(let count=0;count<=70;count++)add('validateLabel',unit.repeat(count));
const surrogateCases=[64,65].map(length=>({length,result:capture(()=>impl.validateLabel('\uD800'.repeat(length)))}));
assert.deepEqual(surrogateCases.map(v=>v.result),[{undefined:true},{error:{name:'CoinbaseStarbaseProtocolError',message:'Starbase label must be at most 64 characters'}}]);
process.stdout.write(JSON.stringify({cases,surrogateCases}));
