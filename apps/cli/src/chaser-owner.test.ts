import type { ExchangeAccountConfig } from '@tealstreet/cli-core';
import type { ExchangeName, Market, Ticker } from '@tealstreet/safe-cex/src/types';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { BaseExchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance as chasers } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { getActiveChasers, getChaserInstance } from '../../../packages/cli-core/src/commands/trading/ChaseCommand';
import { watchOneShotChases } from './chaseAbandoned';
import { ExchangeManager } from './exchange-manager';
import { createStandalonePlatformAdapter } from './platform-adapter';
import { selectStorageOwner } from './storage-context';

vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));

const account: ExchangeAccountConfig = { name: 'Main', exchange: 'bitget', apiKey: 'fixture', apiSecret: 'fixture' };
const options = { symbol: 'FILUSDT', size: 1, side: OrderSide.Buy, maxAttempts: 10, delay: 1000 };
const makeExchange = (name: ExchangeName = 'bitget') => {
  const store = new DefaultStore();
  store.update({
    markets: [{ symbol: 'FILUSDT', precision: { price: 0.001, amount: 1 } } as Market],
    tickers: [{ symbol: 'FILUSDT', bid: 5, ask: 5.001, last: 5 } as Ticker],
  });
  const exchange = new BaseExchange(name, { key: 'fixture', secret: 'fixture' }, store);
  vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
  return exchange;
};

beforeEach(() => vi.useFakeTimers());
afterEach(async () => {
  for (const chase of [...chasers.instances]) chasers.cleanupExchange(chase.exchange);
  await vi.runAllTimersAsync();
  vi.useRealTimers();
});

it.each(['placement', 'settlement'] as const)('drops A at a login switch during %s, preserving B', async (phase) => {
  selectStorageOwner('chaser-owner-A');
  const a = makeExchange(),
    b = makeExchange();
  let release!: (ids: string[]) => void;
  const aPlacement = vi.spyOn(a, 'placeOrder').mockImplementation(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  const bPlacement = vi.spyOn(b, 'placeOrder').mockResolvedValue(['B-order']);
  const realAdapter = createStandalonePlatformAdapter();
  const manager = new ExchangeManager({
    createExchange: vi.fn().mockResolvedValueOnce(a).mockResolvedValueOnce(b),
    startExchange: vi.fn(),
    stopExchange: realAdapter.stopExchange,
    updateAccount: vi.fn(),
  });
  await manager.enable(account);
  const old = new ChaserOrderInstance(a, options);
  chasers.add(old);
  await vi.advanceTimersByTimeAsync(0);
  expect(aPlacement).toHaveBeenCalledTimes(1);
  const oldResult = phase === 'placement' ? watchOneShotChases(a)() : undefined;
  if (phase === 'settlement') {
    release(['A-order']);
    await vi.advanceTimersByTimeAsync(0);
    expect(old.currentOrderIds).toEqual(['A-order']);
  }

  selectStorageOwner('chaser-owner-B');
  expect(a.isDisposed).toBe(true);
  await manager.enable(account);
  expect(manager.get('Main')?.exchange).toBe(b);
  expect(chasers.instances).not.toContain(old);
  expect(getChaserInstance(old.id)).toBeUndefined();
  expect(getActiveChasers()).toEqual([]);
  await expect(watchOneShotChases(b)()).resolves.toBeUndefined();

  const aComplete = vi.fn();
  a.on('chaser:complete', aComplete);
  const current = new ChaserOrderInstance(b, options);
  chasers.add(current);
  if (phase === 'placement') release(['A-late-order']);
  await vi.advanceTimersByTimeAsync(1100);
  expect(chasers.instances).toEqual([current]);
  expect(old.isStopped).toBe(true);
  expect(old.currentOrderIds).toEqual([]);
  expect(aComplete).not.toHaveBeenCalled();
  expect(a.emitter.hasListeners('update')).toBe(false);
  expect(bPlacement).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ symbol: 'FILUSDT', amount: 1, side: OrderSide.Buy }),
  );
  expect(current.currentOrderIds).toEqual(['B-order']);
  expect(current.isStopped).toBe(false);
  if (oldResult) await expect(oldResult).resolves.toBeUndefined();
  await manager.stopAll();
});

it('drops A completion if login changes during its final fill read', async () => {
  selectStorageOwner('chaser-fills-A');
  const a = makeExchange('grvt'),
    b = makeExchange();
  vi.spyOn(a, 'placeOrder').mockResolvedValue(['A-order']);
  let release!: (fills: Map<string, number>) => void;
  const fills = vi.fn(
    () =>
      new Promise<Map<string, number>>((resolve) => {
        release = resolve;
      }),
  );
  Object.assign(a, { fetchFillsForOrderIds: fills });
  const realAdapter = createStandalonePlatformAdapter();
  const manager = new ExchangeManager({
    createExchange: vi.fn().mockResolvedValueOnce(a).mockResolvedValueOnce(b),
    startExchange: vi.fn(),
    stopExchange: realAdapter.stopExchange,
    updateAccount: vi.fn(),
  });
  await manager.enable(account);
  const old = new ChaserOrderInstance(a, { ...options, clientId: 'A-client' });
  chasers.add(old);
  await vi.advanceTimersByTimeAsync(1100);
  chasers.remove(old.id);
  await vi.advanceTimersByTimeAsync(0);
  expect(fills).toHaveBeenCalledExactlyOnceWith(['A-order']);
  selectStorageOwner('chaser-fills-B');
  await manager.enable(account);
  const completion = vi.fn();
  a.on('chaser:complete', completion);
  release(new Map([['A-order', 1]]));
  await vi.advanceTimersByTimeAsync(0);
  expect(completion).not.toHaveBeenCalled();
  await expect(watchOneShotChases(b)()).resolves.toBeUndefined();
  const stale = new ChaserOrderInstance(a, options);
  chasers.add(stale);
  await vi.advanceTimersByTimeAsync(0);
  expect(chasers.instances).not.toContain(stale);
  expect(a.placeOrder).toHaveBeenCalledTimes(1);
  await manager.stopAll();
});
