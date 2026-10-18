const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const declaration = source.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'BybitExchange');
const names = ['getPositionUpdatedTime', 'getPositionReliabilityScore', 'dedupePositionsById'];
const methods = names.map(name => {
  const found = declaration.members.filter(node => node.name?.getText(source) === name);
  assert.equal(found.length, 1, name);
  return found[0].getText(source);
});
const context = vm.createContext({});
vm.runInContext(ts.transpileModule(`globalThis.Subject=class {${methods.join('\n')}};`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText, context);
const row = {id:'p',contracts:2,info:{updatedTime:'1700000000000',seq:7,positionStatus:'Normal',avgPrice:' '}};
const selected = new context.Subject().dedupePositionsById([row,{...row,info:{...row.info,avgPrice:'\ud800'}}]);
assert.equal(selected.length,1);
assert.equal(selected[0].info.avgPrice,'\ud800');
process.stdout.write(JSON.stringify({units:[selected[0].info.avgPrice.charCodeAt(0)]}));
