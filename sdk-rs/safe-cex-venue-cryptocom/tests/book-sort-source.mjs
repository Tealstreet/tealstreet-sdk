import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts, loadSource} from '../../safe-cex-store/tests/source_loader.mjs';

assert.equal(process.versions.node, '24.20.0', 'use the maintained CI Node oracle');
assert.match(process.versions.v8, /^13\.6\./);
const side = process.argv[2];
assert.ok(['bids', 'asks'].includes(side));
const url = new URL('../../../packages/safe-cex/src/exchanges/cryptocom/cryptocom.ws-public.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.filter(m => m.name?.getText(ast) === 'listenOrderBook');
assert.equal(member.length, 1);
const books = loadSource(new URL('../../../packages/safe-cex/src/utils/orderbook.ts', import.meta.url));
const module = {exports: {}};
vm.runInNewContext(ts.transpileModule(`export class Selected {${member[0].getText(ast)}}`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText, {module, exports: module.exports, ...books});
const bits = value => {
  if (Number.isNaN(value)) return 'NaN';
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
};
const profiles = [['0', '-0', '0', '-0'], ['NaN', '2', '1', 'NaN', '3'],
  ['Infinity', '-Infinity', 'Infinity', '0', '-0', '-Infinity']];
for (const length of [22, 32, 63, 64, 65, 95, 127, 128, 129, 257]) {
  profiles.push(Array.from({length}, (_, i) => i % 7 === 0 || i % 11 === 0 ? 'NaN' : String((i * 197 + 558) % 997)));
}
const cases = [];
for (const prices of profiles) {
  const receiver = new module.exports.Selected();
  receiver.isConnected = true;
  receiver.messageHandlers = {};
  receiver.subscribeToOrderBook = () => {};
  const data = {bids: [], asks: [], t: 1};
  data[side] = prices.map((price, i) => [price, String(i + 1)]);
  const update = {instrument_name: 'BTCUSD-PERP', data: [data]};
  let observed;
  let callbacks = 0;
  receiver.listenOrderBook('BTCUSD-PERP', book => {
    callbacks++;
    observed = book[side].map(level => [bits(level.price), bits(level.amount), bits(level.total)]);
  });
  receiver.messageHandlers['book.BTCUSD-PERP'][0](update);
  assert.equal(callbacks, 1);
  assert.equal(observed.length, prices.length);
  cases.push({update, observed});
}
process.stdout.write(JSON.stringify(cases));
