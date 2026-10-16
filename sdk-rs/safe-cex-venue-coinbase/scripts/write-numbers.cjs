const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const {captureHeader,preserveSpecialNumbers,numberBits}=require('./special-numbers.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
const source=ts.createSourceFile('write.ts',fs.readFileSync(path.join(root,'coinbase.starbase-orders.ts'),'utf8'));
const names=['protocolError','requirePositive','coinbaseStarbaseTotalBaseAmount','coinbaseStarbaseTotalContracts','coinbaseStarbaseUnresolvedValidUntil','COINBASE_STARBASE_BASE_AMOUNT_SOURCE'];
const definitions=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(source))));
assert.equal(definitions.length,names.length,'all live numeric implementations selected');
const errorImports=source.statements.filter(ts.isImportDeclaration).flatMap(declaration=>{
 const bindings=declaration.importClause?.namedBindings;return bindings&&ts.isNamedImports(bindings)?Array.from(bindings.elements).filter(s=>s.name.text==='CoinbaseStarbaseProtocolError').map(specifier=>({declaration,specifier})):[];
});
assert.equal(errorImports.length,1,'one protocol error import');
assert.equal(errorImports[0].declaration.moduleSpecifier.text,'./coinbase.starbase');
assert.equal((errorImports[0].specifier.propertyName??errorImports[0].specifier.name).text,'CoinbaseStarbaseProtocolError');
const protocol={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:protocol});
const implementation={};vm.runInNewContext(ts.transpileModule(definitions.map(n=>n.getText(source)).join('\n')+'\nexport {'+names.join(',')+'};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:implementation,CoinbaseStarbaseProtocolError:protocol.CoinbaseStarbaseProtocolError});
function capture(fn){try{return preserveSpecialNumbers({ok:fn()});}catch(error){return {error:{name:error.name,message:error.message}};}}
function bits(value){return numberBits(value);}
const numbers=[0,-0,1,-1,0.5,-0.5,Number.MIN_VALUE,-Number.MIN_VALUE,Number.MAX_VALUE,-Number.MAX_VALUE,NaN,Infinity,-Infinity,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,-Number.MAX_SAFE_INTEGER,-Number.MAX_SAFE_INTEGER-1,2**53,2**53+2,1e-30,1e30];
let seed=0x436f696e62617365n;const mask=(1n<<64n)-1n;
for(let i=0;i<2048;i++){seed^=seed<<13n;seed&=mask;seed^=seed>>7n;seed^=seed<<17n;seed&=mask;const b=Buffer.alloc(8);b.writeBigUInt64LE(seed);numbers.push(b.readDoubleLE());}
for(let i=-16;i<=16;i++)numbers.push(Number.MAX_SAFE_INTEGER+i,-Number.MAX_SAFE_INTEGER+i,i/4);
const contexts=['field','','\uFEFF','\u0085','💰','line\nbreak'];
const cases=[];
for(const input of numbers){
 for(const name of ['coinbaseStarbaseTotalBaseAmount','coinbaseStarbaseTotalContracts','coinbaseStarbaseUnresolvedValidUntil'])cases.push({name,bits:bits(input),result:capture(()=>implementation[name](input))});
 for(const context of contexts)cases.push({name:'requirePositive',bits:bits(input),context,result:capture(()=>implementation.requirePositive(input,context))});
}
for(const input of [undefined,null,true,false,'','0','1','\uFEFF','\u0085',[],[1],{}, {value:1}, {toString:null}])for(const context of contexts)cases.push({name:'requirePositive',nonNumber:true,context,result:capture(()=>implementation.requirePositive(input,context))});
for(const context of contexts)cases.push({name:'protocolError',context,result:capture(()=>implementation.protocolError(context))});
process.stdout.write(JSON.stringify(cases.map(test=>({...test,...captureHeader()}))));
