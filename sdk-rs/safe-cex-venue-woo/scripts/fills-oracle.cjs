const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo/woo.legacy-fills.ts'),'utf8');
const api={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:api});
const inputs=[{limit:100},{limit:100,symbol:''},{limit:100,symbol:'PERP_BTC_USDT',fromMilliseconds:0,toMilliseconds:1,page:0},
 ...['fromMilliseconds','toMilliseconds','page','limit'].flatMap(key=>[-1,0,0.5,1700000000123,1e21].map(value=>({limit:100,[key]:value}))),
 {limit:1,symbol:'\ufeff'},{limit:1,symbol:'\u0085'}];
for(const key of ['from_milliseconds','to_milliseconds','startTime','endTime','size'])inputs.push({limit:100,[key]:7});
const cases=inputs.map(input=>({input,output:api.buildWooLegacyFillsParams(input)}));
const file=path.resolve(__dirname,'../tests/fixtures/fills.json'),bytes=JSON.stringify(cases,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),bytes,'WOO legacy fills query source drift');else fs.writeFileSync(file,bytes);
console.log(`WOO legacy fills params: ${cases.length} live cases`);

const ast=ts.createSourceFile('woo.legacy-fills.ts',source,ts.ScriptTarget.Latest,true);
function inventory(tree) {
 const declaration=tree.statements.find(s=>ts.isInterfaceDeclaration(s)&&s.name.text==='WooLegacyFillsQuery');
 assert.ok(declaration,'WOO fills query interface must exist');
 // Local members cannot claim the accepted set of inherited fields.
 assert.ok(!declaration.heritageClauses?.length,'WOO fills query inherited fields require an explicit inventory');
 return declaration.members.map(m=>({name:m.name.getText(tree),optional:!!m.questionToken,type:m.type.getText(tree)}));
}
const fields=inventory(ast);
const inheritedSource='interface ExtraNumeric { hiddenNumeric: number }\n'+source.replace(
 'interface WooLegacyFillsQuery {','interface WooLegacyFillsQuery extends ExtraNumeric {');
assert.notEqual(inheritedSource.slice(inheritedSource.indexOf('\n')+1),source,'heritage control must change the actual declaration');
assert.throws(()=>inventory(ts.createSourceFile('woo.legacy-fills.ts',inheritedSource,ts.ScriptTarget.Latest,true)),
 /WOO fills query inherited fields require an explicit inventory/,'inherited numeric field must not disappear');
const schema=path.resolve(__dirname,'../tests/fixtures/fills-schema.json'),schemaBytes=JSON.stringify(fields,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(schema,'utf8'),schemaBytes,'WOO fills declaration drift');else fs.writeFileSync(schema,schemaBytes);
