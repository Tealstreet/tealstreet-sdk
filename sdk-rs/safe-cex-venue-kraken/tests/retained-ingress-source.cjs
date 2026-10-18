'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const units = text => Array.from({length: text.length}, (_, i) => text.charCodeAt(i));
const profiles = [
  ['high', '\\ud800', [0xd800]],
  ['low', '\\udc00', [0xdc00]],
  ['pair', '\\ud83d\\ude80', [0xd83d, 0xde80]],
];
const families = [
  ['kraken.ws-public', 'KrakenPublicWebsocket', 'handleBookSnapshot'],
  ['kraken.ws-private', 'KrakenPrivateWebsocket', 'handleOpenOrders'],
  ['krakenspot.ws-public', 'KrakenSpotPublicWebsocket', 'handleTicker'],
  ['krakenspot.ws-private', 'KrakenSpotPrivateWebsocket', 'handleExecutions'],
];
const cases = [];
for (const [family, className, handler] of families) {
  const filename = path.join(root, 'packages/safe-cex/src/exchanges/kraken', family + '.ts');
  const source = fs.readFileSync(filename, 'utf8');
  const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  const cls = ast.statements.filter(s => ts.isClassDeclaration(s) && s.name.text === className);
  assert.equal(cls.length, 1);
  const members = cls[0].members.filter(m => m.name?.getText(ast) === 'onMessage');
  assert.equal(members.length, 1);
  assert.ok(ts.isArrowFunction(members[0].initializer));
  const code = ts.transpileModule('function make(){return (' + members[0].initializer.getText(ast) + ')}', {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  for (const [id, token, expected] of profiles) {
    const order = {order_id: 'retained-order', instrument: 'PF_XBTUSD', symbol: 'XBT/USD',
      type: 'lmt', direction: 0, qty: 3, filled: 0.5, limit_price: '29100', side: 'buy',
      order_type: 'limit', order_qty: '3', cum_qty: '0.5', exec_type: 'new',
      timestamp: '2023-11-14T22:13:20Z', marker: '__unit__'};
    const frame = family === 'kraken.ws-public'
      ? {feed: 'book_snapshot', product_id: 'PF_XBTUSD', timestamp: 1, seq: 1,
          bids: [{price: 30000, qty: 2}], asks: [{price: 30001, qty: 3}], marker: '__unit__'}
      : family === 'kraken.ws-private'
        ? {feed: 'open_orders_snapshot', orders: [order, {...order, order_id: 'neighbor', marker: 'ordinary'}]}
        : {channel: family === 'krakenspot.ws-public' ? 'ticker' : 'executions', type: 'snapshot',
            data: [order, {...order, order_id: 'neighbor', marker: 'ordinary'}]};
    const raw = JSON.stringify(frame).replace('"__unit__"', '"' + token + '"');
    let delivered;
    const receiver = {isDisposed: false, trackLastReceived() {}, recordMessageHandlerStats() {},
      [handler](message) { assert.equal(delivered, undefined); delivered = message; }};
    const errors = [];
    const jsonModule = {exports: {}};
    const jsonFile = path.join(root, 'packages/json-parse/src/jsonParse.ts');
    vm.runInNewContext(ts.transpileModule(fs.readFileSync(jsonFile, 'utf8'), {
      compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
    }).outputText, {exports: jsonModule.exports, JSON});
    const fn = vm.runInNewContext(code + ';make', {
      JSON, jsonParse: jsonModule.exports.jsonParse,
      console: {error(...args) { errors.push(args); }},
    }).call(receiver);
    fn({data: raw});
    assert.equal(errors.length, 0);
    assert.ok(delivered, 'maintained onMessage dispatches the complete frame');
    const row = delivered.orders?.[0] || delivered.data?.[0] || delivered;
    assert.deepEqual(units(row.marker), expected, 'native source units before JSON projection');
    if (delivered.orders || delivered.data) {
      const rows = delivered.orders || delivered.data;
      assert.equal(rows.length, 2);
      assert.equal(rows[1].order_id, 'neighbor');
    }
    cases.push({family, id, raw, units: expected});
  }
}
assert.equal(cases.length, 12);
process.stdout.write(JSON.stringify(cases));
