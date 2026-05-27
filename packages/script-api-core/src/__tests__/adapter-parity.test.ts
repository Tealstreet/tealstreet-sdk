/**
 * Parity test — two CoreAdapter instances fed identical seed must produce
 * identical results through the InProcessProxy. Also asserts every result is
 * `structuredClone`-safe so the POJO contract (SPEC §3) holds from day one.
 *
 * This is type-and-shape parity, not real-store parity. The real
 * standalone-CLI store vs jotai-store integration test is deferred to Phase 2
 * once the action surface lands and gives us a meaningful e2e to validate.
 */
import { describe, expect, it } from 'vitest';

import { InProcessProxy } from '../proxy/InProcessProxy';

import { SEED } from './fixtures';
import { createMockAdapter } from './mockAdapter';

const cloneSeed = () => structuredClone(SEED);

const buildProxies = () => {
  const adapterA = createMockAdapter(SEED);
  const adapterB = createMockAdapter(cloneSeed());
  return {
    proxyA: new InProcessProxy(adapterA),
    proxyB: new InProcessProxy(adapterB),
  };
};

const assertParityAndClone = async (resultA: unknown, resultB: unknown) => {
  expect(resultA).toEqual(resultB);
  expect(() => structuredClone(resultA)).not.toThrow();
};

describe('InProcessProxy adapter parity', () => {
  it('getActiveAccount returns identical strings', async () => {
    const { proxyA, proxyB } = buildProxies();
    const a = await proxyA.call('getActiveAccount', []);
    const b = await proxyB.call('getActiveAccount', []);
    await assertParityAndClone(a, b);
    expect(a).toBe('mybybit');
  });

  it('getActiveAccounts returns identical streaming-account lists', async () => {
    const { proxyA, proxyB } = buildProxies();
    const a = await proxyA.call('getActiveAccounts', []);
    const b = await proxyB.call('getActiveAccounts', []);
    await assertParityAndClone(a, b);
    expect(a).toEqual(['mybybit', 'hl-prod']);
  });

  it('getActiveSymbol + getActiveExchange parity', async () => {
    const { proxyA, proxyB } = buildProxies();
    const aSym = await proxyA.call('getActiveSymbol', []);
    const bSym = await proxyB.call('getActiveSymbol', []);
    await assertParityAndClone(aSym, bSym);

    const aEx = await proxyA.call('getActiveExchange', []);
    const bEx = await proxyB.call('getActiveExchange', []);
    await assertParityAndClone(aEx, bEx);
  });

  it('getAccounts returns identical AccountInfo[]', async () => {
    const { proxyA, proxyB } = buildProxies();
    const a = await proxyA.call('getAccounts', []);
    const b = await proxyB.call('getAccounts', []);
    await assertParityAndClone(a, b);
    expect(a).toHaveLength(2);
  });

  it('getMarkets({ account }) returns identical arrays', async () => {
    const { proxyA, proxyB } = buildProxies();
    const a = await proxyA.call('getMarkets', [{ account: 'mybybit' }]);
    const b = await proxyB.call('getMarkets', [{ account: 'mybybit' }]);
    await assertParityAndClone(a, b);
    expect(a.map((m) => m.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
  });

  it('getMarket(symbol, { account }) parity (hit + miss)', async () => {
    const { proxyA, proxyB } = buildProxies();
    const hitA = await proxyA.call('getMarket', ['BTCUSDT', { account: 'mybybit' }]);
    const hitB = await proxyB.call('getMarket', ['BTCUSDT', { account: 'mybybit' }]);
    await assertParityAndClone(hitA, hitB);
    expect(hitA?.symbol).toBe('BTCUSDT');

    const missA = await proxyA.call('getMarket', ['NONE', { account: 'mybybit' }]);
    const missB = await proxyB.call('getMarket', ['NONE', { account: 'mybybit' }]);
    expect(missA).toBeUndefined();
    expect(missB).toBeUndefined();
  });

  it('getTicker + getTickers parity', async () => {
    const { proxyA, proxyB } = buildProxies();
    const t1A = await proxyA.call('getTicker', ['ETHUSDT', { account: 'mybybit' }]);
    const t1B = await proxyB.call('getTicker', ['ETHUSDT', { account: 'mybybit' }]);
    await assertParityAndClone(t1A, t1B);

    const t2A = await proxyA.call('getTickers', [['BTCUSDT', 'ETHUSDT'], { account: 'mybybit' }]);
    const t2B = await proxyB.call('getTickers', [['BTCUSDT', 'ETHUSDT'], { account: 'mybybit' }]);
    await assertParityAndClone(t2A, t2B);
    expect(t2A).toHaveLength(2);
  });

  it('getPositions parity, all + symbol-filtered', async () => {
    const { proxyA, proxyB } = buildProxies();
    const allA = await proxyA.call('getPositions', [{ account: 'mybybit' }]);
    const allB = await proxyB.call('getPositions', [{ account: 'mybybit' }]);
    await assertParityAndClone(allA, allB);
    expect(allA).toHaveLength(2);

    const btcA = await proxyA.call('getPositions', [{ account: 'mybybit', symbol: 'BTCUSDT' }]);
    const btcB = await proxyB.call('getPositions', [{ account: 'mybybit', symbol: 'BTCUSDT' }]);
    await assertParityAndClone(btcA, btcB);
    expect(btcA).toHaveLength(1);
  });

  it('getHistoricalPosition parity (hit + miss)', async () => {
    const { proxyA, proxyB } = buildProxies();
    const hitA = await proxyA.call('getHistoricalPosition', [{ account: 'mybybit', symbol: 'BTCUSDT' }]);
    const hitB = await proxyB.call('getHistoricalPosition', [{ account: 'mybybit', symbol: 'BTCUSDT' }]);
    await assertParityAndClone(hitA, hitB);
    expect(hitA?.fees).toBe(12.5);

    const missA = await proxyA.call('getHistoricalPosition', [{ account: 'mybybit', symbol: 'XRP' }]);
    expect(missA).toBeUndefined();
  });

  it('getOrders parity', async () => {
    const { proxyA, proxyB } = buildProxies();
    const a = await proxyA.call('getOrders', [{ account: 'mybybit' }]);
    const b = await proxyB.call('getOrders', [{ account: 'mybybit' }]);
    await assertParityAndClone(a, b);
    expect(a).toHaveLength(1);
  });

  it('getBalance + getCombinedBalance parity', async () => {
    const { proxyA, proxyB } = buildProxies();
    const balA = await proxyA.call('getBalance', [{ account: 'mybybit' }]);
    const balB = await proxyB.call('getBalance', [{ account: 'mybybit' }]);
    await assertParityAndClone(balA, balB);
    expect(balA?.total).toBe(100_000);

    const combA = await proxyA.call('getCombinedBalance', [{}]);
    const combB = await proxyB.call('getCombinedBalance', [{}]);
    await assertParityAndClone(combA, combB);
    expect(combA.totalEquity).toBe(110_000);
    expect(combA.longExposure).toBe(49_000);
    expect(combA.shortExposure).toBe(30_100);

    const subA = await proxyA.call('getCombinedBalance', [{ accounts: ['mybybit'] }]);
    const subB = await proxyB.call('getCombinedBalance', [{ accounts: ['mybybit'] }]);
    await assertParityAndClone(subA, subB);
    expect(subA.totalEquity).toBe(100_000);
  });

  it('getOrderbook parity (hit + miss)', async () => {
    const { proxyA, proxyB } = buildProxies();
    const hitA = await proxyA.call('getOrderbook', ['BTCUSDT', { account: 'mybybit' }]);
    const hitB = await proxyB.call('getOrderbook', ['BTCUSDT', { account: 'mybybit' }]);
    await assertParityAndClone(hitA, hitB);
    expect(hitA?.bids[0]?.price).toBe(50000);

    const missA = await proxyA.call('getOrderbook', ['ETHUSDT', { account: 'mybybit' }]);
    expect(missA).toBeUndefined();
  });

  it('getCandles parity (default + limit)', async () => {
    const { proxyA, proxyB } = buildProxies();
    const a = await proxyA.call('getCandles', ['BTCUSDT', '1m', { account: 'mybybit' }]);
    const b = await proxyB.call('getCandles', ['BTCUSDT', '1m', { account: 'mybybit' }]);
    await assertParityAndClone(a, b);
    expect(a).toHaveLength(1);
  });
});
