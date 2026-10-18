import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const file = new URL('exchanges/okx/okx.exchange.ts', root);
const ast = ts.createSourceFile(file.pathname, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'OKXExchange');
const selected = ['updateOrder', 'updateAlgoOrder', 'updateTriggerAlgoOrder', 'toFiniteNumber'].map(name => {
  const rows = cls.members.filter(n => n.name?.getText(ast) === name);
  assert.equal(rows.length, 1, name);
  return rows[0].getText(ast);
}).join('\n');
const getter = ast.statements.filter(ts.isVariableStatement)
  .find(n => n.declarationList.declarations.some(d => d.name.getText(ast) === 'getOKXTriggerPxType'));
assert(getter);
const types = loadSource(new URL('exchanges/okx/okx.types.ts', root), [], ['ENDPOINTS']);
const enums = loadSource(new URL('types.ts', root));
const math = {exports:{}};
const mathCode = ts.transpileModule(fs.readFileSync(new URL('utils/safe-math.ts', root), 'utf8'), {
  compilerOptions: {target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS},
}).outputText;
vm.runInNewContext(mathCode, {module:math, exports:math.exports, require: name => {
  assert.equal(name, '@tealstreet/math/src/isNotNumber');
  return loadSource(new URL('../../math/src/isNotNumber.ts', root));
}});
const code = `${getter.getText(ast)}\nclass Source {${selected}}\nSource`;
const Source = vm.runInNewContext(ts.transpileModule(code, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None},
}).outputText, {...enums, ...math.exports, ...types});
const results = [];
for (const family of ['limit', 'conditional']) {
  for (const shape of ['inherited', 'shadowed', 'absent', 'primitive']) {
    const source = new Source();
    const requests = [];
    source.store = {marketsBySymbol: {BTCUSDT: {id:'BTC-USDT-SWAP', precision:{price:0.25,amount:0.003}}}};
    source.xhr = {post: async (url, data) => {
      requests.push({url, data});
      return {data:{code:'0', data:[{sCode:'0'}]}};
    }};
    source.emitAndLogError = message => {throw Error('Unexpected source emission: '+message);};
    let update = shape === 'primitive' ? 7 : Object.create(shape === 'absent' ? null : {price:104.37, amount:7.9});
    if (shape === 'shadowed') Object.assign(update, {price:102.625, amount:4.1});
    const order = {id:'order-1',symbol:'BTCUSDT',type:family,amount:3,price:100,triggerType:'mark_price'};
    let ids, error;
    try {ids = await source.updateOrder({order, update});}
    catch (caught) {error = {name:caught.name, message:caught.message};}
    results.push({family, shape, requests, ...(error ? {error} : {ids})});
  }
}
assert.equal(results.length, 8);
process.stdout.write(JSON.stringify(results));
