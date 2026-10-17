import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';

const require = createRequire(import.meta.url);
const {map} = require('p-iteration');
const flatten = require('lodash/flatten');
function members(file, owner, names) {
  const url = new URL(`../../../packages/safe-cex/src/exchanges/okx/${file}`, import.meta.url);
  const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === owner);
  return names.map(name => {
    const found = cls.members.filter(n => n.name?.getText(ast) === name);
    assert.equal(found.length, 1, name);
    return found[0].getText(ast);
  }).join('\n');
}
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.types.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const endpoints = ast.statements.find(n => ts.isVariableStatement(n) &&
  n.declarationList.declarations.some(d => d.name.getText(ast) === 'ENDPOINTS'));
const selected = members('okx.exchange.ts', 'OKXExchange', ['fetchNormalOrders', 'fetchAlgoOrders', 'mapOrders']) +
  members('okxeu.exchange.ts', 'OKXEUExchange', ['getInstrumentTypes']);
const module = {exports: {}};
const compiled = ts.transpileModule(`${endpoints.getText(ast)}\nexport class Selected {${selected}}`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
vm.runInNewContext(compiled, {module, exports: module.exports, map, flatten});
const result = [];
for (const [method, endpoint] of [['fetchNormalOrders', 'UNFILLED_ORDERS'], ['fetchAlgoOrders', 'UNFILLED_ALGO_ORDERS']]) {
  const receiver = new module.exports.Selected();
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const requests = [];
  receiver.xhr = {get: async (path, config) => {
    assert.equal(path, module.exports.ENDPOINTS[endpoint]);
    requests.push(config.params.instType);
    if (config.params.instType === 'SWAP') await held;
    return {data: {data: []}};
  }};
  const pending = receiver[method]('oco');
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(requests, ['SWAP', 'FUTURES']);
  result.push({path: module.exports.ENDPOINTS[endpoint], instruments: [...requests]});
  release();
  assert.equal((await pending).length, 0);
}
process.stdout.write(JSON.stringify(result));
