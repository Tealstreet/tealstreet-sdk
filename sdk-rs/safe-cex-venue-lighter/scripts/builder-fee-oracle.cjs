const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = p => fs.readFileSync(path.join(root, p), 'utf8');
function evaluate(source, imports = {}) {
  const exports = {};
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  vm.runInNewContext(js, { exports, require(name) {
    assert.ok(Object.hasOwn(imports, name), `unexpected import ${name}`);
    return imports[name];
  } });
  return exports;
}
const domain = ts.createSourceFile('types.ts', read('packages/safe-cex/src/types.ts'), ts.ScriptTarget.Latest, true);
const enums = evaluate(domain.statements.filter(ts.isEnumDeclaration).map(n => n.getText(domain)).join('\n'));
const types = evaluate(read('packages/safe-cex/src/exchanges/lighter/lighter.types.ts'), {
  '../../types': enums,
  '../../transport/registry': { withPublicCorsProxy() { throw Error('not part of fee oracle'); } },
});
const source = read('packages/safe-cex/src/exchanges/lighter/lighter.builder-fee.ts');
const fees = evaluate(source + '\nexports.parsePositiveIntegerBigInt=parsePositiveIntegerBigInt;exports.resolveMode=resolveMode;exports.getDefaultIntegratorAccountIndex=getDefaultIntegratorAccountIndex;', {'./lighter.types':types});
const tables=Object.fromEntries(Object.entries(fees).filter(([,value])=>typeof value!=='function'));
const ast=ts.createSourceFile('fees.ts',source,ts.ScriptTarget.Latest,true);
const alias=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='LighterBuilderFeeMode');
assert.ok(alias&&ts.isUnionTypeNode(alias.type));
const modes=alias.type.types.map(n=>{assert.ok(ts.isLiteralTypeNode(n)&&ts.isStringLiteral(n.literal));return n.literal.text;});
const inputs=[null,false,true,0,1,'','dynamic','static','Static','STATIC',' static','static ','\uFEFFstatic','static\u0085',['static'],{toString:'static'},{}];
const resolved=inputs.map(input=>({input,expected:fees.resolveMode(input)}));
const missingMode=fees.resolveMode(undefined);
const defaults=[];
for(const exchangeName of [undefined,'lighter','lighter-robinhood','binance'])for(const testnet of [undefined,false,true])defaults.push({exchangeName:exchangeName??null,testnet:testnet??null,expected:fees.getDefaultIntegratorAccountIndex({testnet},exchangeName)??null});
const bigintInputs=[undefined,null,false,true,0,1,9007199254740992,'',' ','0','000','00012','9007199254740993','9'.repeat(400),'-1','+1','0x10','0b10','1e2','1.0','12abc','１２','\\uFEFF12\\uFEFF','\\u008512','12\\n','12\\n3',[],['12'],{toString:null}].map(v=>typeof v==='string'?v.replaceAll('\\uFEFF','\uFEFF').replaceAll('\\u0085','\u0085').replaceAll('\\n','\n'):v);
const bigints=bigintInputs.map(input=>({...input===undefined?{}:{input},expected:fees.parsePositiveIntegerBigInt(input)??null}));
const configs=[];
for(const exchangeName of [undefined,'lighter','lighter-robinhood'])for(const testnet of [false,true])for(const lighterBuilderFeeMode of [undefined,'static','STATIC',null])for(const lighterIntegratorAccountIndex of bigintInputs){
 const options={testnet,extra:{lighterBuilderFeeMode,lighterIntegratorAccountIndex}};
 configs.push({options,exchangeName:exchangeName??null,expected:fees.resolveLighterBuilderFeeConfig(options,exchangeName)??null});
}
const configAlias=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='LighterBuilderFeeConfig');
assert.ok(configAlias&&ts.isTypeLiteralNode(configAlias.type));
const configSchema=configAlias.type.members.map(m=>({name:m.name.text,type:m.type.getText(ast),optional:!!m.questionToken}));
const cardinalityCorpora={resolved,defaults,bigints,configs};
if(process.env.LIGHTER_TYPE_CARDINALITY_MUTATION) {
 const [field,mode]=process.env.LIGHTER_TYPE_CARDINALITY_MUTATION.split(':');
 cardinalityCorpora[field].splice(mode==='empty'?0:1);
}
process.stdout.write(JSON.stringify({tables,modes,resolved,missingMode,defaults,bigints,configs,configSchema},(_key,v)=>typeof v==='bigint'?{$bigint:v.toString()}:v));
