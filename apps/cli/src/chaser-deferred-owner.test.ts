import type { ExchangeAccountConfig } from '@tealstreet/cli-core';
import type { ClientOrderLookup } from '@tealstreet/safe-cex/src/exchanges/clientOrderId';
import type { ExchangeName, Market, Order, Ticker } from '@tealstreet/safe-cex/src/types';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { BaseExchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import { OrderSide, OrderStatus, OrderType } from '@tealstreet/safe-cex/src/types';
import { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance as chasers } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { ExchangeManager } from './exchange-manager';
import { setupExchangeListeners } from './exchange-listeners';
import { createStandalonePlatformAdapter } from './platform-adapter';
import { getStorageOwner, selectStorageOwner } from './storage-context';

vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));

const account: ExchangeAccountConfig = { name: 'Main', exchange: 'bitget', apiKey: 'fixture', apiSecret: 'fixture' };
const options = { symbol: 'FILUSDT', size: 1, side: OrderSide.Buy, maxAttempts: 10, delay: 1000 };
const ticker = (bid: number): Ticker => ({ symbol: 'FILUSDT', bid, ask: bid + 0.001, last: bid }) as Ticker;
const order = (id = 'A-order', price = 5): Order => ({
  id,
  symbol: 'FILUSDT',
  type: OrderType.Limit,
  status: OrderStatus.Open,
  side: OrderSide.Buy,
  price,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 0,
});
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const exchange = (name: ExchangeName) => {
  const store = new DefaultStore();
  store.update({
    markets: [{ symbol: 'FILUSDT', precision: { price: 0.001, amount: 1 } } as Market],
    tickers: [ticker(5)],
  });
  const result = new BaseExchange(name, { key: 'fixture', secret: 'fixture' }, store);
  vi.spyOn(result, 'placeOrder').mockResolvedValueOnce(['A-order']).mockResolvedValue(['A-replacement']);
  vi.spyOn(result, 'cancelOrders').mockResolvedValue();
  return result;
};

async function session(name: ExchangeName = 'bitget') {
  selectStorageOwner('deferred-A');
  const a = exchange(name),
    b = exchange('bitget');
  vi.mocked(b.placeOrder).mockReset().mockResolvedValue(['B-order']);
  const adapter = createStandalonePlatformAdapter();
  const manager = new ExchangeManager({
    createExchange: vi.fn().mockResolvedValueOnce(a).mockResolvedValueOnce(b),
    startExchange: vi.fn(),
    stopExchange: adapter.stopExchange,
    updateAccount: vi.fn(),
  });
  await manager.enable(account);
  const notices: { owner: string | null; type: string; message: string }[] = [];
  const recordFill = vi.fn();
  const info = vi.fn();
  setupExchangeListeners(
    a,
    {
      adapter: { printSuccess: vi.fn(), printError: vi.fn() },
      logger: { info },
      recordFill,
      isMonitorMode: () => true,
      getCurrentAccountName: () => 'Main',
      getHealthMonitor: () => undefined,
      addWatchMessage: (type, message) => notices.push({ owner: getStorageOwner(), type, message }),
    },
    'Main',
  );
  const old = new ChaserOrderInstance(a, options);
  const start = async () => {
    chasers.add(old);
    await vi.advanceTimersByTimeAsync(0);
  };
  const startResting = async () => {
    await start();
    a.store.update({ orders: [order()] });
    await vi.advanceTimersByTimeAsync(1100);
    a.emitter.emit('update', a.store);
    await vi.advanceTimersByTimeAsync(50);
  };
  const switchToB = async () => {
    selectStorageOwner('deferred-B');
    expect(a.isDisposed).toBe(true);
    await manager.enable(account);
    const current = new ChaserOrderInstance(b, options);
    chasers.add(current);
    await vi.advanceTimersByTimeAsync(0);
    expect(chasers.instances).toEqual([current]);
    return current;
  };
  const workingB = () => {
    expect(b.placeOrder).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ symbol: 'FILUSDT', amount: 1 }));
    expect(b.cancelOrders).not.toHaveBeenCalled();
    expect(chasers.instances).toHaveLength(1);
    expect(chasers.instances[0].exchange).toBe(b);
    expect(chasers.instances[0].isStopped).toBe(false);
    expect(chasers.instances[0].remainingSize).toBe(1);
    expect(notices).toEqual([]);
  };
  return { a, b, manager, old, notices, info, recordFill, start, startResting, switchToB, workingB };
}

beforeEach(() => vi.useFakeTimers());
afterEach(async () => {
  for (const chase of [...chasers.instances]) chasers.cleanupExchange(chase.exchange);
  await vi.runAllTimersAsync();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it.each([false, true])('seals production retirement-warning delivery to its owner (active=%s)', async (active) => {
  const s = await session();
  const placement = deferred<string[]>();
  vi.mocked(s.a.placeOrder).mockReset().mockReturnValue(placement.promise);
  if (active) await s.start();
  await s.switchToB();
  placement.resolve(['A-late']);
  await vi.advanceTimersByTimeAsync(1100);
  s.a.emitter.emit('exec-log', { message: 'A late command output' });
  s.a.emitter.emit('fill', { orderId: 'A-order', side: OrderSide.Buy, amount: 0.5, symbol: 'FILUSDT', price: 5 });
  expect(s.info).not.toHaveBeenCalled();
  expect(s.recordFill).not.toHaveBeenCalled();
  s.workingB();
  await s.manager.stopAll();
});

it('retains the normal same-owner disposal warning', async () => {
  const s = await session();
  await s.startResting();
  await s.manager.stopAll();
  expect(s.notices).toEqual([
    {
      owner: 'deferred-A',
      type: 'warning',
      message: 'Chaser for FILUSDT was stopped because the exchange connection was disposed',
    },
  ]);
});

it.each(['cancel', 'fills'] as const)('fences active %s success and failure continuations', async (phase) => {
  for (const retire of [false, true]) {
    for (const reject of [false, true]) {
      const s = await session('grvt');
      const cancel = deferred<void>(),
        read = deferred<Map<string, number>>();
      const fills = vi.fn().mockResolvedValue(new Map([['A-order', 0.25]]));
      Object.assign(s.a, { fetchFillsForOrderIds: fills });
      await s.startResting();
      if (phase === 'cancel') vi.mocked(s.a.cancelOrders).mockImplementationOnce(() => cancel.promise);
      else fills.mockImplementationOnce(() => read.promise);
      s.a.store.update({ tickers: [ticker(6)] });
      await vi.advanceTimersByTimeAsync(50);
      if (phase === 'cancel') expect(fills).not.toHaveBeenCalled();
      else expect(fills).toHaveBeenCalledExactlyOnceWith(['A-order']);
      const apply = vi.spyOn(s.old, 'setFilledAmount');
      if (retire) await s.switchToB();
      const emit = vi.spyOn(s.a.emitter, 'emit');
      if (phase === 'cancel') {
        if (reject) cancel.reject(new Error('cancel fixture refused'));
        else cancel.resolve();
      } else if (reject) read.reject(new Error('fill fixture refused'));
      else read.resolve(new Map([['A-order', 0.25]]));
      await vi.advanceTimersByTimeAsync(1100);
      if (retire) {
        if (phase === 'cancel') expect(fills).not.toHaveBeenCalled();
        expect(apply).not.toHaveBeenCalled();
        expect(s.a.placeOrder).toHaveBeenCalledTimes(1);
        expect(emit.mock.calls.filter(([op]) => ['error', 'warning', 'chaser:complete'].includes(op))).toEqual([]);
        s.workingB();
      } else if (reject) {
        expect(s.notices.some(({ type }) => type === 'error')).toBe(true);
        expect(s.old.isStopped).toBe(true);
      } else {
        expect(fills).toHaveBeenCalledWith(['A-order']);
        expect(apply).toHaveBeenCalledWith('A-order', 0.25);
        expect(s.a.placeOrder).toHaveBeenNthCalledWith(2, expect.objectContaining({ amount: 0.75, price: 6 }));
      }
      await s.manager.stopAll();
      await vi.runAllTimersAsync();
    }
  }
});

it.each(['retry', 'lookup-delay', 'lookup-reply', 'private-ws'] as const)('pins ownership during %s', async (phase) => {
  for (const retire of [false, true]) {
    const s = await session(phase === 'private-ws' ? 'bybitdemo' : 'bitget');
    const lookupReply = deferred<ClientOrderLookup>();
    const lookup = vi.spyOn(s.a, 'findOrderByClientOrderId').mockResolvedValue({ found: true, order: order() });
    if (phase === 'retry') {
      vi.spyOn(s.a, 'mintClientOrderId').mockReturnValue(undefined);
      vi.mocked(s.a.placeOrder).mockReset().mockResolvedValueOnce([]).mockResolvedValue(['A-order']);
    } else if (phase.startsWith('lookup')) {
      vi.spyOn(s.a, 'mintClientOrderId').mockReturnValue('A-client');
      vi.mocked(s.a.placeOrder).mockReset().mockRejectedValue(new Error('placement timed out'));
      if (phase === 'lookup-reply') lookup.mockImplementationOnce(() => lookupReply.promise);
    }
    await s.start();
    await vi.advanceTimersByTimeAsync(phase === 'lookup-reply' ? 1000 : 50);
    expect(s.a.placeOrder).toHaveBeenCalledTimes(phase === 'private-ws' ? 0 : 1);
    expect(lookup).toHaveBeenCalledTimes(phase === 'lookup-reply' ? 1 : 0);
    if (retire) await s.switchToB();
    if (phase === 'lookup-reply') lookupReply.resolve({ found: true, order: order() });
    if (phase === 'private-ws' && !retire) {
      s.a.store.update({
        socketHealth: {
          publicInitialized: true,
          publicHealthy: true,
          privateInitialized: true,
          privateHealthy: true,
        },
      });
    }
    await vi.advanceTimersByTimeAsync(2100);
    if (retire) {
      expect(s.a.placeOrder).toHaveBeenCalledTimes(phase === 'private-ws' ? 0 : 1);
      expect(lookup).toHaveBeenCalledTimes(phase === 'lookup-reply' ? 1 : 0);
      expect(s.old.currentOrderIds).toEqual([]);
      expect(s.a.emitter.hasListeners('update')).toBe(false);
      s.workingB();
    } else {
      expect(s.a.placeOrder).toHaveBeenCalledTimes(phase === 'retry' ? 2 : 1);
      expect(s.old.currentOrderIds).toEqual(['A-order']);
      expect(s.old.isStopped).toBe(false);
    }
    await s.manager.stopAll();
    await vi.runAllTimersAsync();
  }
});

it.each(['ticker', 'orders'] as const)('drops a queued %s check when its owner retires', async (phase) => {
  for (const retire of [false, true]) {
    const s = await session();
    await s.startResting();
    if (phase === 'ticker') {
      s.a.store.update({ tickers: [ticker(5)] });
      await vi.advanceTimersByTimeAsync(0);
      s.a.store.update({ tickers: [ticker(6)] });
    } else {
      const cancel = deferred<void>();
      vi.mocked(s.a.cancelOrders).mockImplementationOnce(() => cancel.promise);
      s.a.store.update({ tickers: [ticker(6)] });
      await vi.advanceTimersByTimeAsync(50);
      expect(s.old.pendingOrdersUpdate).toBe(true);
      s.a.store.update({ orders: [{ ...order(), lastUpdate: 1 }], tickers: [ticker(7)] });
      await vi.advanceTimersByTimeAsync(0);
      cancel.resolve();
      await vi.advanceTimersByTimeAsync(0);
      s.a.store.update({ orders: [order('A-replacement', 6)] });
      await vi.advanceTimersByTimeAsync(1000);
    }
    const placed = phase === 'ticker' ? 1 : 2;
    expect(s.a.placeOrder).toHaveBeenCalledTimes(placed);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    if (retire) await s.switchToB();
    const cancels = vi.mocked(s.a.cancelOrders).mock.calls.length;
    await vi.advanceTimersByTimeAsync(1100);
    if (retire) {
      expect(s.a.placeOrder).toHaveBeenCalledTimes(placed);
      expect(s.a.cancelOrders).toHaveBeenCalledTimes(cancels);
      s.workingB();
    } else {
      expect(s.a.placeOrder).toHaveBeenCalledTimes(placed + 1);
      expect(s.a.placeOrder).toHaveBeenLastCalledWith(
        expect.objectContaining({ amount: 1, price: phase === 'ticker' ? 6 : 7 }),
      );
    }
    await s.manager.stopAll();
    await vi.runAllTimersAsync();
  }
});

it.each(['cleanup', 'late-placement'] as const)('fences the multi-ID %s cancel continuation', async (phase) => {
  for (const retire of [false, true]) {
    const s = await session();
    const placement = deferred<string[]>(),
      cancel = deferred<void>();
    vi.mocked(s.a.placeOrder).mockReset().mockReturnValue(placement.promise);
    vi.mocked(s.a.cancelOrders).mockImplementationOnce(() => cancel.promise);
    await s.start();
    if (phase === 'cleanup') {
      placement.resolve(['A-1', 'A-2']);
      await vi.advanceTimersByTimeAsync(1100);
    }
    chasers.remove(s.old.id);
    if (phase === 'late-placement') placement.resolve(['A-1', 'A-2']);
    await vi.advanceTimersByTimeAsync(0);
    expect(s.a.cancelOrders).toHaveBeenCalledExactlyOnceWith([expect.objectContaining({ id: 'A-1' })]);
    if (retire) await s.switchToB();
    cancel.resolve();
    await vi.advanceTimersByTimeAsync(1100);
    if (retire) {
      expect(s.a.cancelOrders).toHaveBeenCalledTimes(1);
      s.workingB();
    } else {
      expect(s.a.cancelOrders).toHaveBeenCalledTimes(2);
      expect(s.a.cancelOrders).toHaveBeenLastCalledWith([expect.objectContaining({ id: 'A-2' })]);
    }
    await s.manager.stopAll();
    await vi.runAllTimersAsync();
  }
});
