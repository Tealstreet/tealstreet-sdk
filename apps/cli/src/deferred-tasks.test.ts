import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { EventEmitter } from 'node:events';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { Market } from '@tealstreet/safe-cex/src/types';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import { deferred, sessionFixture } from './deferred-test.fixture';
vi.setConfig({ testTimeout: 30_000 });
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-tasks-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(async () => {
  (await import('@tealstreet/cli-core/src/triggers/triggerRegistry')).clearTriggers();
  (await import('@tealstreet/safe-cex/src/webworker/twap/TwapManager')).twapManager.runningTwaps.clear();
  const { chaserOrderManagerInstance } =
    await import('@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager');
  for (const instance of [...chaserOrderManagerInstance.instances])
    chaserOrderManagerInstance.removeKeepOrder(instance.id);
  vi.useRealTimers();
  vi.unstubAllEnvs();
});
const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { price: 1, amount: 1 },
  limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 100 } },
};
it.each(['same', 'B', 'away and back'])(
  'invalidates an already queued trigger while its predecessor is held: %s',
  async (change) => {
    const { storage, login } = await sessionFixture();
    await import('./session-tasks');
    const registry = await import('@tealstreet/cli-core/src/triggers/triggerRegistry');
    const held = deferred(),
      entered = deferred(),
      runs: string[] = [],
      settled = vi.fn();
    const arm = (action: string, run: () => Promise<void>) =>
      storage.runInStorageSession(() =>
        registry.armTrigger({
          describe: 'fixture',
          action,
          symbol: 'BTCUSDT',
          account: 'Main',
          condition: { describe: 'fixture', check: () => true },
          resolveExchange: () => ({ name: 'fixture', emitter: { on() {}, off() {}, emit() {} } }),
          run,
          onSettled: settled,
        }),
      );
    arm('first', async () => {
      entered.resolve();
      await held.promise;
    });
    arm('second', async () => {
      storage.assertStorageSession();
      runs.push('A-second');
    });
    await vi.advanceTimersByTimeAsync(250);
    await entered.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
      arm('new', async () => {
        runs.push('current');
      });
    }
    held.resolve();
    await vi.advanceTimersByTimeAsync(250);
    expect(runs).toEqual(change === 'same' ? ['A-second'] : ['current']);
    expect(settled).toHaveBeenCalledTimes(change === 'same' ? 2 : 1);
  },
);
it.each(['same', 'B', 'away and back'])(
  'does not resurrect a real TWAP after its held placement settles: %s',
  async (change) => {
    const { login } = await sessionFixture();
    await import('./session-tasks');
    const { TwapInstance } = await import('@tealstreet/safe-cex/src/webworker/twap/TwapInstance');
    const { twapManager } = await import('@tealstreet/safe-cex/src/webworker/twap/TwapManager');
    const held = deferred(),
      entered = deferred(),
      placeOrder = vi.fn(async () => {
        entered.resolve();
        await held.promise;
        return ['A-order'];
      });
    // The real supervisor consumes this small lifecycle/transport surface.
    const exchange = { placeOrder, emitter: new EventEmitter() } as unknown as Exchange;
    const worker = { postMessage: vi.fn() } as unknown as Worker;
    const opts = {
      accountName: 'Main',
      activeMarket: market,
      size: 2,
      duration: 1,
      ordersCount: 2,
      side: OrderSide.Buy,
      chaserDelay: 1000,
      reduceOnly: false,
      randomnessPercentage: 0,
      useLimitOrders: false,
      useChaserOrders: false,
    };
    const a = new TwapInstance(exchange, worker, opts, vi.fn());
    twapManager.add(a);
    const pending = a.start();
    await entered.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    held.resolve();
    await pending;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(placeOrder).toHaveBeenCalledTimes(change === 'same' ? 2 : 1);
    expect(twapManager.runningTwaps.has(a.id)).toBe(false);
  },
);
it.each(['same', 'B', 'away and back'])(
  'stops the real chaser during placement without restarting its old supervision: %s',
  async (change) => {
    const { login } = await sessionFixture();
    await import('./session-tasks');
    const { ChaserOrderInstance } = await import('@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance');
    const { chaserOrderManagerInstance: manager } =
      await import('@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager');
    const held = deferred(),
      entered = deferred(),
      emitter = new EventEmitter();
    const placeOrder = vi.fn(async () => {
      entered.resolve();
      await held.promise;
      return ['A-order'];
    });
    const ticker = { symbol: 'BTCUSDT', bid: 100, ask: 101, last: 100 };
    const exchange = {
      name: 'bybit',
      emitter,
      store: {
        positions: [],
        orders: [],
        markets: [market],
        tickers: [ticker],
        marketsBySymbol: { BTCUSDT: market },
        tickersBySymbol: { BTCUSDT: ticker },
        updateIds: { orders: 0, positions: 0 },
      },
      placeOrder,
      cancelOrders: vi.fn(async () => {}),
      on: emitter.on.bind(emitter),
      off: emitter.off.bind(emitter),
    } as unknown as Exchange;
    const a = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 3,
      delay: 10_000,
    });
    manager.add(a);
    await entered.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    held.resolve();
    await vi.advanceTimersByTimeAsync(1000);
    expect(placeOrder).toHaveBeenCalledTimes(1);
    expect(manager.instances.includes(a)).toBe(change === 'same');
    expect(a.isStopped).toBe(change !== 'same');
  },
);
