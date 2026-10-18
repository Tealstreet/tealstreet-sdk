'use strict';
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const base = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid');
const compile = text => ts.transpileModule(text, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const text = fs.readFileSync(path.join(base, 'hyperliquid.types.ts'), 'utf8');
const ast = ts.createSourceFile('hyperliquid.types.ts', text, ts.ScriptTarget.Latest, true);
const declarations = ast.statements.filter(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(item => ts.isIdentifier(item.name) && ['ENDPOINTS','INFO_TYPES'].includes(item.name.text)));
assert.equal(declarations.length, 2, 'two actual maintained constant declarations');
const constants = {};
vm.runInNewContext(compile(declarations.map(node => node.getText(ast)).join('\n')), {exports: constants});
const output = {};
vm.runInNewContext(compile(fs.readFileSync(path.join(base, 'hyperliquid.rateLimiter.ts'), 'utf8')), {exports:output,require:name=>{assert.equal(name,'./hyperliquid.types');return constants;},setTimeout:()=>{throw Error('weight witness must not schedule');}});
const row = JSON.parse(fs.readFileSync(path.join(root, 'sdk-rs/safe-cex-parity/accepted-set/contracts/safe-cex-venue-hyperliquid.json')))['safe_cex_venue_hyperliquid::rate_limit::get_hyperliquid_rest_request_weight'];
assert.equal(row.nativeObservation.cases.length, 51);
for (const {input, output:expected} of row.nativeObservation.cases) {
  let actual;
  try {
    const weight = output.getHyperliquidRestRequestWeight({url:input.url,data:input.data});
    const bits = Buffer.alloc(8); bits.writeDoubleBE(weight);
    actual = {ok:true,weightText:String(weight),weightBits:bits.toString('hex')};
  } catch(error) { actual = {ok:false,name:error.name,message:error.message}; }
  assert.deepEqual(actual, expected, input.case);
}
console.log('51 independently authored native expectations match maintained TS weight/coercion behavior.');
