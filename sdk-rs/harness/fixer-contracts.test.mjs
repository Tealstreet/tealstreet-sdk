import assert from 'node:assert/strict';
import { test } from 'node:test';
import { auditFixerChanges, FIXER_CONTRACTS } from './fixer-contracts.mjs';

const audit = (venue, original, change) => {
  const before = structuredClone(original), after = structuredClone(original);
  change(after);
  const frozenBefore = structuredClone(before), frozenAfter = structuredClone(after);
  const result = auditFixerChanges(venue, before, after);
  assert.deepEqual(before, frozenBefore, 'audit must not mutate the original snapshot');
  assert.deepEqual(after, frozenAfter, 'audit must not mutate the corrected snapshot');
  return result;
};
const account = () => ({
  account: {
    positions: [{ symbol: 'BTCUSDT', side: 'long', marginSize: 0, realizedPnl: 12 }],
    orders: [{ id: 'a', symbol: 'BTCUSDT', type: 'limit', status: 'open', price: 10 }],
    fills: [{ id: 'a', symbol: 'BTCUSDT', side: 'buy', amount: 1, price: 10 }],
    settings: [{ symbol: 'BTCUSDT', leverage: 10, marginType: 'cross' }],
    balance: { total: 100, used: 10, upnl: 1 },
  },
});

test('fixer contracts are deeply immutable exact leaf lists', () => {
  assert.ok(Object.isFrozen(FIXER_CONTRACTS));
  for (const value of Object.values(FIXER_CONTRACTS)) {
    assert.ok(Object.isFrozen(value));
    assert.ok(Object.isFrozen(value.fields));
    assert.ok(Object.isFrozen(value.arrays));
    for (const field of value.fields) assert.ok(!field.includes('*'), field);
    for (const rule of value.arrays) {
      assert.ok(Object.isFrozen(rule));
      if (rule.removedFields) assert.ok(Object.isFrozen(rule.removedFields));
    }
  }
  assert.throws(() => FIXER_CONTRACTS.binance.fields.push('account.balance.total'), TypeError);
});

test('unchanged output needs no permission, including unknown venues', () => {
  for (const venue of ['paradex', 'missing', 'constructor', '__proto__'])
    assert.deepEqual(audit(venue, account(), () => {}), { errors: [], counts: {}, total: 0 });
});

test('a permitted margin repair is counted once and cannot rewrite another account field', () => {
  const result = audit('binance', account(), (output) => { output.account.positions[0].marginSize = 25; });
  assert.deepEqual(result, { errors: [], counts: { 'account.positions[0].marginSize': 1 }, total: 1 });
  const unexpected = audit('binance', account(), (output) => {
    output.account.positions[0].realizedPnl = 0;
    output.account.balance.total = 200;
  });
  assert.equal(unexpected.errors.length, 2);
  assert.equal(unexpected.total, 2);
});

test('permissions do not apply to another venue or a similarly named nested field', () => {
  assert.ok(audit('bybitspot', account(), (output) => { output.account.balance.total = 200; }).errors.length);
  assert.ok(audit('binance', account(), (output) => { output.account.positions[0].extra = { marginSize: 2 }; }).errors.length);
  assert.ok(audit('unregistered', account(), (output) => { output.account.positions[0].marginSize = 2; }).errors.length);
});

test('all changed rows count, rather than one distinct permission name', () => {
  const original = account();
  original.account.positions.push({ symbol: 'ETHUSDT', side: 'short', marginSize: 0, realizedPnl: 2 });
  const result = audit('binance', original, (output) => {
    for (const row of output.account.positions) row.marginSize = 5;
  });
  assert.equal(result.total, 2);
  assert.equal(Object.keys(result.counts).length, 2);
  assert.deepEqual(result.errors, []);
});

test('missing, undefined and null remain distinct changed leaves', () => {
  for (const original of [{}, { account: { positions: [{ marginSize: undefined }] } }]) {
    const result = audit('binance', original, (output) => {
      if (!output.account) output.account = { positions: [{ marginSize: undefined }] };
      else output.account.positions[0].marginSize = null;
    });
    assert.ok(result.total > 0);
  }
  const result = auditFixerChanges('binance', { account: { positions: [{}] } }, { account: { positions: [{ marginSize: undefined }] } });
  assert.deepEqual(result.counts, { 'account.positions[0].marginSize': 1 });
  assert.deepEqual(result.errors, []);
  assert.equal(auditFixerChanges('binance', { account: { positions: [{ marginSize: undefined }] } }, { account: { positions: [{}] } }).total, 1);
});

test('empty objects and arrays cannot disappear without a recorded structural change', () => {
  for (const [before, after] of [[{ account: {} }, {}], [{ account: [] }, {}], [{ account: {} }, { account: [] }]]) {
    const result = auditFixerChanges('binance', before, after);
    assert.ok(result.errors.length);
    assert.equal(result.counts['account@type'], 1);
  }
  const result = auditFixerChanges('kucoin', { commands: [{ error: 'Not implemented' }] }, { commands: [{ ok: [] }] });
  assert.deepEqual(result.errors, []);
  assert.equal(result.counts['commands[0].ok@type'], 1);
});

test('array append/remove permissions are separate from row field permissions', () => {
  assert.ok(audit('binance', account(), (output) => {
    output.account.positions.push({ marginSize: 2 });
  }).errors.some((error) => error.includes('@length')));
  const phemex = audit('phemex', account(), (output) => {
    output.account.settings.push({ symbol: 'ETHUSDT', leverage: 5, marginType: 'isolated' });
  });
  assert.deepEqual(phemex.errors, []);
  assert.equal(phemex.counts['account.settings@length'], 1);
  assert.ok(audit('phemex', account(), (output) => { output.account.settings = []; }).errors.some((error) => error.includes('@length')));
});

test('empty and nested array container changes respect the declared direction', () => {
  for (const [venue, before, after] of [
    ['phemex', { account: { settings: [] } }, { account: {} }],
    ['phemex', { account: { settings: [] } }, {}],
    ['orderly', { account: {} }, { account: { orders: [] } }],
    ['orderly', {}, { account: { orders: [] } }],
  ]) {
    const result = auditFixerChanges(venue, before, after);
    assert.ok(result.errors.length, JSON.stringify([venue, before, after]));
  }
  assert.deepEqual(auditFixerChanges('phemex', { account: {} }, { account: { settings: [] } }).errors, []);
  assert.deepEqual(auditFixerChanges('orderly', { account: { orders: [] } }, { account: {} }).errors, []);
});

test('primitive request/subscription array edits have explicit permissions', () => {
  const result = audit('bybit', { requests: ['GET a'], subscriptions: ['a'] }, (output) => {
    output.requests = ['GET b', 'GET c'];
    output.subscriptions = [];
  });
  assert.deepEqual(result.errors, []);
  assert.ok(result.total >= 4);
  assert.ok(audit('binance', { requests: ['GET a'] }, (output) => { output.requests = []; }).errors.length);
});

test('MEXC may add expected subscriptions but never drop an observed one', () => {
  const added = audit('mexc', { subscriptions: ['b', 'c'] }, (output) => { output.subscriptions = ['a', 'b', 'c']; });
  assert.deepEqual(added.errors, []);
  assert.equal(added.counts['subscriptions@added'], 1);
  for (const [before, after] of [[['a'], []], [['a', 'b'], ['a', 'X']], [['a', 'b'], ['a', 'c', 'd']], [['a', 'a'], ['a', 'm']]]) {
    const result = audit('mexc', { subscriptions: before }, (output) => { output.subscriptions = after; });
    assert.ok(result.errors.some((error) => error.includes('@dropped')), JSON.stringify([before, after]));
  }
  assert.ok(audit('mexc', { subscriptions: ['a'] }, (output) => { output.subscriptions = ['a', 1]; }).errors.length);
  // A hole must not free an own-key slot for an unaudited property.
  const holed = ['a', , 'b'];
  holed.x = { smuggled: 1 };
  assert.ok(auditFixerChanges('mexc', { subscriptions: ['a', 'b'] }, { subscriptions: holed }).errors.length);
  assert.equal(audit('mexc', { subscriptions: ['b', 'c'] }, (output) => { output.subscriptions = ['c', 'b']; }).counts['subscriptions@order'], 1);
  assert.ok(audit('mexc', { requests: ['GET a'] }, (output) => { output.requests = []; }).errors.length);
});

test('reconstructed fill rows permit only their declared historical leaves', () => {
  const result = audit('phemex', account(), (output) => {
    output.account.fills = [{ id: 'b', symbol: 'ETHUSDT', side: 'sell', amount: 2, price: 20 }];
  });
  assert.deepEqual(result.errors, []);
  assert.equal(result.total, 5);
  const metadata = account();
  Object.assign(metadata.account.fills[0], { executionId: 'exec', fee: 0.1, timestamp: 100 });
  const erased = audit('phemex', metadata, (output) => {
    output.account.fills = [{ id: 'a', symbol: 'BTCUSDT', side: 'buy', amount: 1, price: 10 }];
  });
  assert.equal(erased.errors.length, 3);
  for (const field of ['executionId', 'fee', 'timestamp'])
    assert.ok(erased.errors.some((error) => error.includes(`.${field}`)));
});

test('even removing an entire fill cannot silently erase observed metadata', () => {
  const original = account();
  Object.assign(original.account.fills[0], { executionId: 'observed', fee: 0.1, timestamp: 100 });
  const result = audit('cryptocom', original, (output) => { output.account.fills = []; });
  for (const field of ['executionId', 'fee', 'timestamp'])
    assert.ok(result.errors.some((error) => error.includes(`.${field}`)));
});

test('Orderly terminal row removals are distinct from retained row rewrites', () => {
  const original = account();
  original.account.orders.push({ id: 'b', symbol: 'ETHUSDT', status: 'open', type: 'limit', price: 20, takeProfit: 30, parentId: 'parent' });
  const result = audit('orderly', original, (output) => { output.account.orders.shift(); });
  assert.deepEqual(result.errors, []);
  assert.equal(result.counts['account.orders@length'], 1);
  // `id` is allowed to be removed with a terminal row; it is never rewritable.
  assert.ok(audit('orderly', original, (output) => { output.account.orders[0].id = 'rewritten'; }).errors.length);
  assert.ok(audit('orderly', original, (output) => { output.account.orders.push({ id: 'c', type: 'limit' }); }).errors.length);
});

test('removing a row cannot hide an undeclared leaf or nested raw payload', () => {
  const original = account();
  original.account.orders[0].info = { secret: 'must stay visible' };
  const result = audit('orderly', original, (output) => { output.account.orders = []; });
  assert.ok(result.errors.some((error) => error.includes('.info.secret')));
});

test('same-length keyed replacement and reordering are structural changes', () => {
  const original = account();
  original.account.orders.push({ id: 'b', symbol: 'ETHUSDT', status: 'open', type: 'limit', price: 20 });
  assert.ok(audit('orderly', original, (output) => { output.account.orders.reverse(); }).errors.some((error) => error.includes('@order')));
  assert.ok(audit('orderly', original, (output) => { output.account.orders[0] = { id: 'c', type: 'limit' }; }).errors.some((error) => error.includes('@row')));
});

test('duplicate keyed rows cannot make another row disappear from the audit', () => {
  const original = account();
  original.account.orders.push({ ...original.account.orders[0], price: 20 });
  const result = audit('orderly', original, (output) => {
    output.account.orders.pop();
    output.account.orders[0].amount = 500;
  });
  assert.ok(result.errors.some((error) => error.includes('.amount')));
});

test('map permissions match exactly one key, never a recursive subtree', () => {
  const before = { account: { balance: { coins: {} } } };
  const good = audit('kraken', before, (output) => { output.account.balance.coins.BTC = { amount: 1, notional: 10 }; });
  assert.deepEqual(good.errors, []);
  const bad = audit('kraken', before, (output) => { output.account.balance.coins.BTC = { nested: { amount: 1 } }; });
  assert.ok(bad.errors.some((error) => error.includes('.nested.amount')));
});

test('book levels have explicit array structure and numeric-depth leaf permissions', () => {
  const result = audit('bingx', { books: { BTCUSDT: { bids: [[10, 1]], asks: [[11, 1]] } } }, (output) => {
    output.books.BTCUSDT.bids.push([9, 2]);
    output.books.BTCUSDT.asks[0][1] = 3;
  });
  assert.deepEqual(result.errors, []);
  assert.ok(audit('bingx', { books: { BTCUSDT: { bids: [[10, 1]], asks: [] } } }, (output) => {
    output.books.BTCUSDT.bids[0].hidden = 1;
  }).errors.length);
});

test('command fields are enumerated and do not authorize raw/unknown subtrees', () => {
  for (const venue of ['coinbase', 'orderly', 'kucoin']) {
    const result = audit(venue, { commands: [{ ok: [] }] }, (output) => {
      output.commands[0] = { ok: [{ id: 'fill', timestamp: 100, symbol: 'BTCUSDT', orderId: 'order', price: 1, qty: 2, side: 'buy', commission: 0.1, maker: true }] };
    });
    assert.deepEqual(result.errors, [], venue);
    const bad = audit(venue, { commands: [{ ok: [] }] }, (output) => { output.commands[0] = { ok: [{ info: { price: 1 } }] }; });
    assert.ok(bad.errors.length, venue);
    assert.ok(audit(venue, { commands: [{ ok: [] }] }, (output) => { output.commands.push({ ok: [] }); }).errors.length, venue);
  }
});

test('unkeyed query row reordering needs an explicit structural permission', () => {
  const a = { id: 'a', price: 1 }, b = { id: 'b', price: 2 }, c = { id: 'c', price: 3 };
  for (const venue of ['kucoin', 'coinbase', 'orderly']) {
    const swapped = auditFixerChanges(venue, { commands: [{ ok: [a, b] }] }, { commands: [{ ok: [b, a] }] });
    assert.ok(swapped.errors.some((error) => error.includes('commands[0].ok@order')), venue);
    const mixed = auditFixerChanges(venue, { commands: [{ ok: [a, b, c] }] }, { commands: [{ ok: [c, a] }] });
    assert.ok(mixed.errors.some((error) => error.includes('commands[0].ok@order')), venue);
  }
  const fillsBefore = { account: { fills: [{ id: 'a', price: 1 }, { id: 'b', price: 2 }] } };
  const fillsAfter = { account: { fills: [...fillsBefore.account.fills].reverse() } };
  assert.deepEqual(auditFixerChanges('phemex', fillsBefore, fillsAfter).errors, []);
});

test('scalar null command acknowledgments require their own narrow permission', () => {
  for (const venue of ['lighter', 'lighter-robinhood', 'coinbase'])
    assert.deepEqual(audit(venue, { commands: [{ error: 'Not implemented' }] }, (output) => { output.commands[0] = { ok: null }; }).errors, []);
  assert.ok(audit('lighter', { commands: [{ error: 'Not implemented' }] }, (output) => { output.commands[0] = { ok: 'arbitrary result' }; }).errors.length);
  assert.ok(audit('kucoin', { commands: [{ error: 'Not implemented' }] }, (output) => { output.commands[0] = { ok: null }; }).errors.length);
});

test('registered aliases use the same deeply frozen contract', () => {
  for (const alias of ['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])
    assert.equal(FIXER_CONTRACTS[alias], FIXER_CONTRACTS.orderly);
  assert.equal(FIXER_CONTRACTS.coinbase, FIXER_CONTRACTS.coinbasespot);
  assert.equal(FIXER_CONTRACTS.lighter, FIXER_CONTRACTS['lighter-robinhood']);
});

test('sparse array additions and removals cannot exploit empty leaves', () => {
  const result = audit('bybit', { requests: ['a'] }, (output) => { output.requests.length = 3; });
  assert.ok(result.errors.some((error) => error.includes('@hole')));
  const hole = audit('bybit', { requests: ['a', 'b'] }, (output) => { delete output.requests[0]; });
  assert.ok(hole.errors.some((error) => error.includes('@hole')));
  const row = audit('binance', { account: { positions: [{ marginSize: 1 }] } }, (output) => { delete output.account.positions[0]; });
  assert.ok(row.errors.some((error) => error.includes('@row')));
});

test('non-JSON containers and cycles fail closed rather than appearing empty', () => {
  for (const value of [new Date(), new Map(), new Set(), new Uint8Array(1)]) {
    const result = auditFixerChanges('binance', { account: { positions: [{ marginSize: 0 }] } }, { account: { positions: [{ marginSize: value }] } });
    assert.ok(result.errors.some((error) => error.includes('@unsupported')));
  }
  const cyclic = {}; cyclic.self = cyclic;
  assert.ok(auditFixerChanges('missing', {}, cyclic).errors.length);
});

test('symbol properties and prototype-looking object keys cannot hide changes', () => {
  const symbol = Symbol('hidden');
  assert.ok(auditFixerChanges('binance', {}, { [symbol]: 1 }).errors.length);
  const before = JSON.parse('{"account":{"balance":{"coins":{}}}}');
  const after = JSON.parse('{"account":{"balance":{"coins":{"__proto__":{"amount":1,"notional":2}}}}}');
  const result = auditFixerChanges('kraken', before, after);
  assert.deepEqual(result.errors, []);
  assert.equal({}.amount, undefined);
});
