import type { Exchange } from '../../exchanges/base';
import type { Position } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../../types';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { UpdatePositionsHandler } from '../../webworker/handlers/UpdatePositionsHandler';

const position = (symbol = 'BTCUSDT'): Position => ({
  id: symbol,
  symbol,
  side: PositionSide.Long,
  contracts: 2,
  entryPrice: 100,
  notional: 200,
  unrealizedPnl: 3,
  leverage: 7,
  liquidationPrice: 60,
  marginType: MarginType.Isolated,
  positionMode: PositionMode.OneWay,
});
const order = {
  id: 'order-1',
  symbol: 'BTCUSDT',
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 100,
  amount: 2,
  filled: 0,
  remaining: 2,
  reduceOnly: false,
  lastUpdate: 1_700_000_000_000,
};
const cleanups: Array<() => void> = [];

const bridge = (hostedMode: boolean) => {
  const pending: any[] = [];
  const delivered: any[] = [];
  // Retain the pre-clone object so a wire event can land between the omit
  // decision and structured cloning, as in the documented race.
  const ctx = { postMessage: (message: unknown) => pending.push(message) } as unknown as Worker;
  const worker = new ExchangeWorker(
    ctx,
    'account-A',
    'bybit',
    { key: '', secret: '', hostedMode },
    {} as any,
    {} as any,
    'session-A',
  );
  const main = new DefaultStore();
  const fetchPositions = vi.fn().mockResolvedValue([]);
  const exchange = { store: worker.store, isDisposed: false, fetchPositions } as unknown as Exchange;
  const handler = new UpdatePositionsHandler('account-A', 'bybit', exchange, ctx, {} as any, {} as any);
  const unsubscribe = worker.store.subscribe((data) => (worker as any)._throttledStoreUpdate(data));
  const deliver = () => {
    while (pending.length) {
      const message = structuredClone(pending.shift());
      delivered.push(message);
      if (message.op !== 'store') continue;
      for (const key of ['orders', 'positions'] as const) {
        const incoming = message.data.updateIds?.[key];
        if (incoming !== undefined && incoming !== main.updateIds[key]) {
          expect(Array.isArray(message.data[key]), `advancing ${key} must carry its array`).toBe(true);
        }
      }
      main.update(message.data, true);
    }
  };
  const flush = () => {
    vi.advanceTimersByTime(250);
    deliver();
  };
  cleanups.push(() => {
    unsubscribe();
    handler.dispose();
    worker.dispose();
  });
  return { worker, main, handler, fetchPositions, pending, delivered, deliver, flush };
};

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-10T12:00:00Z'));
});
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup());
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe.each([false, true])('closed-position worker bridge (hosted=%s)', (hosted) => {
  it('Worker Position Refresh Bridge clears omitted rows on main for partial and empty FULL snapshots', async () => {
    const b = bridge(hosted);
    b.worker.store.addOrUpdatePositionsFull([position(), position('ETHUSDT')]);
    b.flush();
    expect(b.main.positions.map((row) => row.id)).toEqual(['BTCUSDT', 'ETHUSDT']);
    const firstId = b.main.updateIds.positions;
    b.fetchPositions.mockResolvedValue([position('ETHUSDT')]);
    await b.handler.handleUpdatePositions('partial-full');
    b.flush();
    expect(b.main.positions.map((row) => row.id)).toEqual(['ETHUSDT']);
    expect(b.main.updateIds.positions).toBeGreaterThan(firstId);
    expect(b.main.positionSettings.BTCUSDT.leverage).toBe(7);
    b.fetchPositions.mockResolvedValue([]);
    await b.handler.handleUpdatePositions('empty-full');
    b.flush();
    expect(b.worker.store.positions).toEqual([]);
    expect(b.main.positions).toEqual([]);
    expect(b.main.serialize().positions).toEqual([]);
    expect(b.main.updateIds.positions).toBe(b.worker.store.updateIds.positions);
    expect(b.delivered.filter((m) => m.op === 'store').at(-1).data.positions).toEqual([]);
    expect(Object.keys(b.main.positionSettings).sort()).toEqual(['BTCUSDT', 'ETHUSDT']);
    // A late snapshot must also clear a consumer still holding old exposure.
    const late = new DefaultStore();
    late.addOrUpdatePositionsFull([position()]);
    b.worker.sendSnapshot();
    const snapshot = structuredClone(b.pending.find((m) => m.op === 'store').data);
    expect(snapshot.positions).toEqual([]);
    late.update(snapshot, true);
    expect(late.positions).toEqual([]);
  });

  it('Update-id contract survives the structured-clone race when positions close and orders cancel', async () => {
    const b = bridge(hosted);
    b.worker.store.update({ orders: [order], positions: [position()] });
    b.flush();
    const before = { ...b.main.updateIds };
    b.worker.store.update({ balance: { ...b.worker.store.balance } });
    vi.advanceTimersByTime(250);
    const omitted = b.pending.find((m) => m.op === 'store').data;
    expect(omitted).not.toHaveProperty('positions');
    expect(omitted).not.toHaveProperty('orders');
    // Full-account close and terminal order arrive after the omit decisions,
    // before the older message has undergone structured cloning/delivery.
    await b.handler.handleUpdatePositions('close-during-clone');
    b.worker.store.addOrUpdateOrders([{ ...order, status: OrderStatus.Canceled }]);
    expect(omitted.updateIds).toEqual(before);
    expect(b.worker.store.updateIds.positions).toBeGreaterThan(before.positions);
    expect(b.worker.store.updateIds.orders).toBeGreaterThan(before.orders);
    b.deliver();
    b.flush();
    expect(b.main.positions).toEqual([]);
    expect(b.main.orders[0].status).toBe(OrderStatus.Canceled);
    expect(b.main.updateIds).toEqual(b.worker.store.updateIds);
    // Future unchanged payloads may omit arrays only at those same IDs.
    b.worker.store.update({ balance: { ...b.worker.store.balance } });
    b.flush();
    expect(b.main.positions).toEqual([]);
    expect(b.main.orders[0].status).toBe(OrderStatus.Canceled);
  });
});
