import { test } from 'node:test';
import assert from 'node:assert/strict';
import { finalizeReplay } from './replay-finalize.mjs';

test('replay freezes every comparison table before synchronous and asynchronous memory collection', async () => {
  const balance = {
    used: 138, free: 1162, total: 1280, upnl: 20,
    coins: { BTC: { base: 'BTC', amount: 1, notional: 100 } },
    currencies: { USDT: { total: 1080, free: 962, used: 138, upnl: 20 } },
  };
  const store = {
    markets: [{ symbol: 'BTCUSDT', precision: { amount: 0.001, price: 0.1 }, limits: { amount: { max: Infinity } } }],
    tickers: [{ symbol: 'BTCUSDT', last: 70000 }],
    balance,
    orders: [{ id: 'order', amount: 1, extra: { remainingNative: 1 }, clientOrderId: undefined }],
    positions: [{ symbol: 'BTCUSDT', contracts: 1, info: { currency: 'USDT' } }],
  };
  const books = { BTCUSDT: { bids: [{ price: 69999, amount: 2 }], asks: [] } };
  const settings = [{ symbol: 'BTCUSDT', buyLeverage: 10, sellLeverage: 20 }];
  const fills = [{ id: 'execution', amount: 0.1 }];
  const commands = [{ ok: { ids: ['accepted'] } }];
  const writes = ['accepted-write'];
  const protocolErrors = [];
  // Deliberately retain aliases, as live Store rows and a venue's tsSettings
  // hook do. A shallow copy of the balance or orders must fail this regression.
  const capture = () => ({
    frames: 14,
    store: { markets: store.markets, tickers: store.tickers },
    books,
    account: { balance: store.balance, orders: store.orders, positions: store.positions, settings, fills },
    commands, writes, protocolErrors,
  });
  const expected = structuredClone(capture());
  const events = [];
  const result = await finalizeReplay(() => {
    events.push('capture');
    return capture();
  }, async () => {
    events.push('measure');
    // A synchronous collection callback must also run after the boundary.
    store.balance.total = 1300;
    store.balance.currencies.USDT.total = 1100;
    await new Promise((resolve) => setImmediate(() => {
      store.markets[0].precision.price = 1;
      store.markets[0].limits.amount.max = 10;
      store.tickers[0].last = 1;
      store.balance.coins.BTC.amount = 9;
      store.orders[0].extra.remainingNative = 0;
      store.orders.push({ id: 'later' });
      store.positions[0].info.currency = 'USDC';
      settings[0].sellLeverage = 99;
      fills[0].amount = 99;
      books.BTCUSDT.bids[0].amount = 99;
      commands[0].ok.ids.push('late');
      writes.push('late-write');
      protocolErrors.push('late-audit');
      resolve();
    }));
    events.push('settled');
    return { cpuMs: 5, retainedBytes: 100 };
  });
  assert.deepEqual(events, ['capture', 'measure', 'settled']);
  assert.deepEqual(result, { ...expected, metrics: { cpuMs: 5, retainedBytes: 100 } });
  assert.equal(store.balance.total, 1300, 'memory settling actually mutated the live Store');
  assert.equal(result.account.balance.total, 1280);
  assert.equal(result.store.markets[0].limits.amount.max, Infinity);
  assert.equal(Object.hasOwn(result.account.orders[0], 'clientOrderId'), true);
});

test('public and native typed-array snapshots stay detached without adding account state', async () => {
  const native = new Float64Array([70000, 2]);
  const result = await finalizeReplay(() => ({ store: { markets: [], tickers: [] }, books: { BTC: native } }), async () => {
    native[0] = 1;
    await Promise.resolve();
    native[1] = 99;
    return { cpuMs: 1 };
  });
  assert.deepEqual([...result.books.BTC], [70000, 2]);
  assert.equal(Object.hasOwn(result, 'account'), false);
});

test('snapshot and measurement failures remain visible to the replay', async () => {
  let measured = false;
  await assert.rejects(finalizeReplay(() => { throw new Error('capture failed'); }, () => {
    measured = true;
  }), /capture failed/);
  assert.equal(measured, false);
  await assert.rejects(finalizeReplay(() => ({}), async () => { throw new Error('memory failed'); }), /memory failed/);
});
