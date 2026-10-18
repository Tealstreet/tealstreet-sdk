import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts, loadSource} from '../../safe-cex-store/tests/source_loader.mjs';

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
const result = [];
for (const side of ['bids', 'asks']) {
  const receiver = new module.exports.Selected();
  receiver.isConnected = true;
  receiver.messageHandlers = {};
  receiver.subscribeToOrderBook = () => {};
  const update = {instrument_name: 'BTCUSD-PERP', data: [{
    bids: [['30000', '1']], asks: [['30001', '2']], t: 1,
  }]};
  update.data[0][side] = [{'0': '30000', '1': '1'}];
  let callbacks = 0;
  receiver.listenOrderBook('BTCUSD-PERP', () => { callbacks++; });
  let error;
  try {
    receiver.messageHandlers['book.BTCUSD-PERP'][0](update);
  } catch (cause) {
    error = {name: cause.name, message: cause.message};
  }
  assert.equal(error?.name, 'TypeError');
  assert.match(error.message, /iterable/);
  assert.equal(callbacks, 0);
  result.push({side, update, callbacks, error});
}
process.stdout.write(JSON.stringify(result));
