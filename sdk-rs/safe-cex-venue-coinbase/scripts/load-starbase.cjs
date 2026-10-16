// Execute inert source modules with real bindings; only the public symbol helpers
// are sliced to avoid loading unrelated book operations into these pure oracles.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
function run(code,bindings={}){
 const exports={};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:name=>{assert(Object.hasOwn(bindings,name),'unclassified source import '+name);return bindings[name];}});return exports;
}
const protocol=run(fs.readFileSync(path.join(root,'coinbase.starbase.ts'),'utf8'));
const typeSource=ts.createSourceFile('types.ts',fs.readFileSync(path.join(root,'../../types.ts'),'utf8'));
const types=run(typeSource.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(typeSource)).join('\n'));
const publicSource=ts.createSourceFile('public.ts',fs.readFileSync(path.join(root,'coinbase.starbase-public.ts'),'utf8'));
const publicNames=['STARBASE_BASE_CURRENCIES','parseBaseCurrency','parseStarbasePerpetualName','toCoinbaseStarbaseInstrumentName','toCoinbaseLegacyIntxSymbol','requireString','protocolError'];
const definitions=publicSource.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>publicNames.includes(d.name.getText(publicSource))));assert.equal(definitions.length,publicNames.length);
const imports=publicSource.statements.filter(n=>ts.isImportDeclaration(n)&&n.moduleSpecifier.text==='./coinbase.starbase');assert.equal(imports.length,1);
const publicApi=run([...imports,...definitions].map(n=>n.getText(publicSource)).join('\n'),{'./coinbase.starbase':protocol});
const bindings={'../../types':types,'./coinbase.starbase':protocol,'./coinbase.starbase-public':publicApi};
const privateApi=run(fs.readFileSync(path.join(root,'coinbase.starbase-private.ts'),'utf8'),bindings);
const writeCode=fs.readFileSync(path.join(root,'coinbase.starbase-orders.ts'),'utf8');
const write=run(writeCode+'\nexport {decodeOrderIdentity, buildOtoco, buildOtocoLeg, validateOtocoLegShape, validateOrderShape};',{...bindings,'./coinbase.starbase-private':privateApi});
module.exports={write,privateApi,types,ts,writeCode};
