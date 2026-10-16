import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';

const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'OKXExchange');
const names = ['isAlgoOrder', 'getAlgoOrderPlacementPriority'];
const members = names.map(name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, `maintained order policy ${name}`);
  return member.getText(ast);
});
const {OrderType} = loadSource(new URL('../../../packages/safe-cex/src/types.ts', import.meta.url));
const inventory = ['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'];
assert.deepEqual(Object.values(OrderType), inventory, 'ORDER-POLICY source enum inventory');
const module = {exports: {}};
const code = ts.transpileModule('export class Selected {' + members.join('\n') + '}', {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
vm.runInThisContext('(function(module,exports,OrderType){' + code + '})')(module, module.exports, OrderType);
const policy = new module.exports.Selected();
const cases = inventory.map(type => ({type, algo: policy.isAlgoOrder(type), priority: policy.getAlgoOrderPlacementPriority(type)}));
assert.equal(cases.length, 6, 'ORDER-POLICY source total');
process.stdout.write(JSON.stringify({capture: {platform: process.platform, architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0])}, cases}));
