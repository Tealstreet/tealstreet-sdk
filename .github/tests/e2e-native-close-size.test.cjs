const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { stripTypeScriptTypes } = require('node:module');
const { resolve } = require('node:path');
const test = require('node:test');
const { runInNewContext } = require('node:vm');

const source = stripTypeScriptTypes(readFileSync(resolve(__dirname, '../../apps/web/cypress/e2e/spec.cy.ts'), 'utf8'));
const declaration = (name) => {
  const start = source.indexOf(`const ${name} =`);
  assert.ok(start >= 0, `locate ${name}`);
  const end = source.indexOf('\n};', start);
  assert.ok(end > start, `locate the end of ${name}`);
  return source.slice(start, end + 3);
};
const sizeHelper = declaration('nativeCloseSize');
const position = (contractsNative) => ({
  symbol: 'BTCUSDT',
  side: 'long',
  contracts: 4,
  notional: 60000,
  extra: { contractsNative },
  market: { contractSize: 0.01, isInverse: false, precision: { amount: 1 } },
});
const exchange = (contractsNative) => ({ store: { displayedPositions: [position(contractsNative)] } });
const nativeSize = (exch) =>
  runInNewContext(`${sizeHelper}; nativeCloseSize(exch, 'BTCUSDT', 'long')`, {
    exch,
    adjust: (amount) => Math.floor(amount),
  });

test('close size is the exact native value, independent of derived size, precision and market metadata', () => {
  const exch = exchange(1.375);
  assert.equal(nativeSize(exch), 1.375);
  exch.store.displayedPositions[0].market.isInverse = true;
  assert.equal(nativeSize(exch), 1.375);
  delete exch.store.displayedPositions[0].market;
  assert.equal(nativeSize(exch), 1.375);
});

test('unknown or invalid native sizes refuse even when base and notional sizes are available', () => {
  for (const value of [undefined, null, 0, -1, NaN, Infinity, -Infinity, '2', '']) {
    assert.equal(nativeSize(exchange(value)), undefined, `native size ${String(value)}`);
  }
  assert.equal(nativeSize({ store: { displayedPositions: [] } }), undefined);
  assert.equal(nativeSize({ store: {} }), undefined);
  const otherSide = exchange(2);
  otherSide.store.displayedPositions[0].side = 'short';
  assert.equal(nativeSize(otherSide), undefined);
});

function runClose(name, contractsNative) {
  const orders = [];
  const logs = [];
  const pending = [];
  const exch = exchange(contractsNative);
  exch.placeOrder = async (order) => {
    orders.push(order);
    return ['synthetic-order'];
  };
  const accountName = 'synthetic-account';
  const eac = { getExchanges: () => new Map([[accountName, exch]]) };
  const win = {
    exchangeAccountController: eac,
    __getStoreSnapshot: () => ({ [accountName]: { positions: [position(contractsNative)] } }),
  };
  const cy = {
    window: () => ({
      then: (callback) => {
        const result = Promise.resolve().then(() => callback(win));
        pending.push(result);
        return result;
      },
    }),
    wrap: (value) => Promise.resolve(value),
    task: (_name, message) => {
      logs.push(message);
    },
  };
  runInNewContext(`${sizeHelper}; ${declaration(name)}; ${name}('long');`, {
    cy,
    symbol: 'BTCUSDT',
    exchangeConfig: { exchange: 'paradex' },
    getParadexExchangeForE2E: () => ({ accountName, eac, exch }),
    PositionSide: { Long: 'long' },
    OrderSide: { Buy: 'buy', Sell: 'sell' },
    OrderTimeInForce: { ImmediateOrCancel: 'ImmediateOrCancel' },
    adjust: (amount) => Math.floor(amount),
  });
  return { done: Promise.all(pending), orders, logs };
}

test('cleanup logs a refusal and sends no close order when native size is unknown', async () => {
  const result = runClose('closeAllPositionsViaController', undefined);
  await result.done;
  assert.equal(result.orders.length, 0);
  assert.ok(result.logs.some((line) => /skipped.*native.*(?:unknown|unavailable|invalid)/i.test(line)));
});

test('cleanup sends the exact native size when known', async () => {
  const result = runClose('closeAllPositionsViaController', 1.375);
  await result.done;
  assert.equal(result.orders.length, 1);
  assert.equal(result.orders[0].amount, 1.375);
  assert.equal(result.orders[0].reduceOnly, true);
});

test('Paradex close fails explicitly before submitting an unknown native size', async () => {
  const result = runClose('closePositionWithMarketOrderForE2E', undefined);
  await assert.rejects(result.done, /native.*(?:unknown|unavailable|invalid)/i);
  assert.equal(result.orders.length, 0);
});

test('Paradex close submits the exact native size when known', async () => {
  const result = runClose('closePositionWithMarketOrderForE2E', 1.375);
  await result.done;
  assert.equal(result.orders.length, 1);
  assert.equal(result.orders[0].amount, 1.375);
  assert.equal(result.orders[0].reduceOnly, true);
});
