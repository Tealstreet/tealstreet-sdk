import { describe, expect, it, vi } from 'vitest';

import type { Store } from '@tealstreet/safe-cex/src/store/store.interface';
import type { Market, PlaceOrderOpts, Position } from '@tealstreet/safe-cex/src/types';

import { createCoreAdapter } from '../adapter/createCoreAdapter';
import type { CoreAdapter } from '../adapter/AdapterContext';
import type { HostBindings, HostExchange, HostWriteEvent } from '../adapter/HostBindings';
import { CORE_API_VERSION } from '../version';
import { runScript, type ScriptRecord } from './runScript';

const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
};

const fixture = () => {
  const market = {
    symbol: 'BTCUSDT',
    precision: { amount: 0.1, price: 0.5 },
    limits: { amount: { min: 0, max: 0 } },
    isSpot: false,
  } as Market;
  const store = (size: number) =>
    ({
      markets: [market],
      tickers: [],
      orders: [{ id: 'resting', symbol: 'BTCUSDT', status: 'open', side: 'buy', price: 60_000 }],
      positions: [
        { symbol: 'BTCUSDT', side: 'long', contracts: size, notional: 120_000 },
        { symbol: 'BTCUSDT', side: 'short', contracts: size / 2, notional: 60_000 },
      ] as Position[],
    }) as unknown as Store;
  const aWrite = vi.fn(async (_order: PlaceOrderOpts) => ['a-id']);
  const bWrite = vi.fn(async (_order: PlaceOrderOpts) => ['b-id']);
  const a: HostExchange = { name: 'binance', store: store(2), placeOrder: aWrite };
  const b: HostExchange = { name: 'binance', store: store(20), placeOrder: bWrite };
  let current = a;
  const writes: HostWriteEvent[] = [];
  const host: HostBindings = {
    getExchange: () => current,
    getStore: () => current.store,
    readActiveAccount: () => 'main',
    readActiveSymbol: () => 'BTCUSDT',
    readStreamingAccounts: () => ['main'],
    readAllAccounts: () => [{ name: 'main', exchange: 'binance' }],
    readOrderbook: () => undefined,
    onWrite: (event) => writes.push(event),
  };
  return { host, a, aWrite, bWrite, writes, replace: () => (current = b) };
};

const order = { symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1 } as const;
const record: ScriptRecord = {
  id: 'owner-fence',
  name: 'owner-fence',
  targets: ['core'],
  capabilities: {
    accounts: ['main'],
    writes: ['placeOrder'],
    idempotent: false,
    storage: { enabled: false, quotaBytes: 0 },
  },
};

describe('script account-owner continuity', () => {
  for (const phase of ['account lookup', 'transpile', 'body'] as const) {
    it.each([false, true])(`checks owner after deferred ${phase} (replace=%s)`, async (replace) => {
      const f = fixture();
      const adapter = createCoreAdapter(f.host);
      const entered = deferred<void>();
      const resume = deferred<void>();
      const wait = async () => {
        entered.resolve();
        await resume.promise;
      };
      const pending = runScript(
        phase === 'transpile'
          ? { ...record, code: `await api.exchange.placeOrder(${JSON.stringify(order)});` }
          : {
              ...record,
              fn: async (_args, api) => {
                if (phase === 'body') await wait();
                await api.exchange.placeOrder(order);
              },
            },
        '',
        {
          adapter:
            phase === 'account lookup' ? { ...adapter, getActiveAccount: async () => (await wait(), 'main') } : adapter,
          transpile: async (source) => {
            await wait();
            return source;
          },
          runtime: { mountedTargets: ['core'], coreApiVersion: CORE_API_VERSION },
          invokedFrom: 'cli',
        },
      );
      await entered.promise;
      if (replace) f.replace();
      resume.resolve();
      const result = await pending;
      expect(f.bWrite).not.toHaveBeenCalled();
      expect(f.aWrite).toHaveBeenCalledTimes(replace ? 0 : 1);
      expect(result.ok).toBe(!replace);
      if (replace) expect(result.error).toMatch(/owner changed/);
    });
  }

  it.each([false, true])('checks owner between hedge-close legs (replace=%s)', async (replace) => {
    const f = fixture();
    const entered = deferred<void>();
    const first = deferred<string[]>();
    f.aWrite.mockImplementationOnce(() => {
      entered.resolve();
      return first.promise;
    });
    const adapter = createCoreAdapter(f.host);
    const pending = adapter.closePosition({ symbol: 'BTCUSDT' }, 'main');
    await entered.promise;
    if (replace) f.replace();
    first.resolve(['a-first']);
    if (replace) {
      await expect(pending).rejects.toThrow(/owner changed/);
      expect(f.aWrite).toHaveBeenCalledTimes(1);
      expect(f.bWrite).not.toHaveBeenCalled();
      expect(f.writes).toEqual([]);
      await adapter.closePosition({ symbol: 'BTCUSDT' }, 'main');
      expect(f.bWrite.mock.calls.map(([sent]) => sent.amount)).toEqual([20, 10]);
    } else {
      expect(await pending).toEqual({ orderIds: ['a-first', 'a-id'] });
      expect(f.aWrite.mock.calls.map(([sent]) => sent.amount)).toEqual([2, 1]);
      expect(f.writes).toMatchObject([{ method: 'closePosition', ok: true, orderIds: ['a-first', 'a-id'] }]);
    }
  });
});

const ordinaryWrites = {
  placeOrder: (a: CoreAdapter) => a.placeOrder(order, 'main'),
  placeOrders: (a: CoreAdapter) => a.placeOrders([order], 'main'),
  cancelOrder: (a: CoreAdapter) => a.cancelOrder('resting', { account: 'main' }),
  cancelOrders: (a: CoreAdapter) => a.cancelOrders({}, 'main'),
  updateOrder: (a: CoreAdapter) => a.updateOrder('resting', { update: { price: 60_000 } }, 'main'),
  setLeverage: (a: CoreAdapter) => a.setLeverage('BTCUSDT', 3, 'main'),
  setPositionMode: (a: CoreAdapter) => a.setPositionMode('hedged', 'main'),
  setMarginType: (a: CoreAdapter) => a.setMarginType('BTCUSDT', 'cross', 'main'),
  runCommand: (a: CoreAdapter) => a.runCommand('buy 1', { account: 'main', depth: 1 }),
};

describe('ordinary write publication ownership', () => {
  for (const [method, invoke] of Object.entries(ordinaryWrites)) {
    for (const failed of [false, true]) {
      it.each([false, true])(`${method} completion (failed=${failed}, replace=%s)`, async (replace) => {
        const f = fixture();
        const reply = deferred<unknown>();
        const native = vi.fn(() => reply.promise);
        Object.assign(f.a, {
          placeOrder: native,
          placeOrders: native,
          cancelOrders: native,
          updateOrder: native,
          setLeverage: native,
          changePositionMode: native,
          changeMarginType: native,
        });
        const adapter = createCoreAdapter({ ...f.host, runCommand: native as never });
        const pending = Promise.resolve(invoke(adapter)).then(
          (value) => ({ value, error: undefined }),
          (error: Error) => ({ value: undefined, error }),
        );
        expect(native).toHaveBeenCalledTimes(1);
        if (replace) f.replace();
        if (failed) reply.reject(new Error('original venue failure'));
        else reply.resolve(method === 'runCommand' ? { ok: true, output: [], capturedIds: [] } : ['a-completed']);
        const result = await pending;
        expect(native).toHaveBeenCalledTimes(1);
        expect(f.bWrite).not.toHaveBeenCalled();
        if (replace) {
          expect(f.writes).toEqual([]);
          expect(result.error?.message).toMatch(/owner changed/);
          expect(result.value).toBeUndefined();
        } else {
          expect(result.error?.message).toBe(failed ? 'original venue failure' : undefined);
          expect(f.writes).toHaveLength(method === 'runCommand' ? 0 : 1);
          if (method !== 'runCommand') expect(f.writes[0]).toMatchObject({ method, ok: !failed });
        }
      });
    }
  }
});

describe('script result publication ownership', () => {
  for (const phase of ['placement reply', 'after completed placement'] as const) {
    it.each([false, true])(`${phase} (replace=%s)`, async (replace) => {
      const f = fixture();
      const entered = deferred<void>();
      const resume = deferred<void>();
      const print = vi.fn();
      f.host.print = print;
      f.aWrite.mockImplementationOnce(async () => {
        if (phase === 'placement reply') {
          entered.resolve();
          await resume.promise;
        }
        return ['a-completed'];
      });
      const pending = runScript(
        {
          ...record,
          fn: async (_args, api) => {
            const result = await api.exchange.placeOrder(order);
            if (phase === 'after completed placement') {
              entered.resolve();
              await resume.promise;
            }
            api.io.print(result.orderIds[0]);
            api.io.return(result.orderIds[0]);
          },
        },
        '',
        {
          adapter: createCoreAdapter(f.host),
          transpile: async (source) => source,
          runtime: { mountedTargets: ['core'], coreApiVersion: CORE_API_VERSION },
          invokedFrom: 'cli',
        },
      );
      await entered.promise;
      if (replace) f.replace();
      resume.resolve();
      const result = await pending;
      expect(f.aWrite).toHaveBeenCalledTimes(1);
      expect(f.bWrite).not.toHaveBeenCalled();
      if (replace) {
        if (phase === 'placement reply') expect(f.writes).toEqual([]);
        expect(print).not.toHaveBeenCalled();
        expect(result).toMatchObject({ ok: false, output: [], error: expect.stringMatching(/owner changed/) });
        expect(result.returnValue).toBeUndefined();
      } else {
        expect(print).toHaveBeenCalledWith('a-completed', 'print');
        expect(result).toMatchObject({ ok: true, returnValue: 'a-completed' });
        expect(f.writes).toMatchObject([{ method: 'placeOrder', ok: true, orderIds: ['a-completed'] }]);
      }
    });
  }
});
