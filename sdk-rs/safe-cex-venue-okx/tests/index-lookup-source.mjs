import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';

const source = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts', import.meta.url);
const ast = ts.createSourceFile(source.pathname, fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'OKXExchange');
const members = ['getIndexTickerInstId', 'getMarketsForIndexTickerInstId'].map(name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, name);
  return member.getText(ast);
});
const code = ts.transpileModule(`export class Selected {${members.join('\n')}}`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const module = {exports: {}};
vm.runInThisContext(`(function(module,exports){${code}\n})`)(module, module.exports);
const receiver = new module.exports.Selected();

let unrelatedReads = 0;
const row = {id: 'BTC-USDT-SWAP', info: {uly: 'BTC-USDT'}};
Object.defineProperty(row, 'limits', {enumerable: true, get() {
  unrelatedReads++;
  throw new Error('unrelated limits must stay unread');
}});
receiver.store = {markets: [row]};
const selected = receiver.getMarketsForIndexTickerInstId('BTC-USDT');
assert.equal(unrelatedReads, 0);
assert.equal(selected[0], row);
selected[0].marker = true;
assert.equal(row.marker, true);

const rows = [{marker: 1}, , null];
receiver.store = {markets: rows};
let calls = 0;
receiver.getIndexTickerInstId = function(value) {
  assert.equal(this, receiver);
  calls++;
  rows.push('not visited');
  receiver.getIndexTickerInstId = () => 7;
  return value.marker;
};
assert.deepEqual(receiver.getMarketsForIndexTickerInstId(7), [null]);
assert.equal(calls, 1);
assert.deepEqual(receiver.getMarketsForIndexTickerInstId('7'), []);
delete receiver.getIndexTickerInstId;

assert.equal(receiver.getIndexTickerInstId({info: {uly: '\ud800'}}), '\ud800');
const id = {replace(pattern, replacement) {
  assert.equal(this, id);
  assert.equal(pattern.source, '-SWAP$');
  assert.equal(replacement, '');
  return 'custom';
}};
assert.equal(receiver.getIndexTickerInstId({id, info: null}), 'custom');
for (const suffix of ['', '\n', '\r', '\r\n', '\u2028', '\u2029']) {
  const id = `\ud800-SWAP${suffix}`;
  assert.equal(receiver.getIndexTickerInstId({id}), suffix ? id : '\ud800');
}
const original = {cause: 'opaque'};
receiver.store = {markets: [{get info() {throw original;}}, null]};
assert.throws(() => receiver.getMarketsForIndexTickerInstId('x'), error => error === original);
const customRows = [null];
const sentinel = {};
customRows.filter = function(callback) {
  assert.equal(this, customRows);
  assert.equal(typeof callback, 'function');
  return sentinel;
};
receiver.store = {markets: customRows};
assert.equal(receiver.getMarketsForIndexTickerInstId(null), sentinel);
console.log('OKX actual-source index lookup: fields, identity, dynamic dispatch, holes, UTF16, throws PASS');
