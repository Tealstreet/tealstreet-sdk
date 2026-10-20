import { afterEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Market, Order } from '../../types';
import { BaseExchange } from '../../exchanges/base';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType, OrderStopTriggerType } from '../../types';
import { ChaserOrderInstance } from '../../webworker/chaserOrder/ChaserOrderInstance';
import { ChaserOrderManager } from '../../webworker/chaserOrder/ChaserOrderManager';
import { consumerFrame } from '../cutover/consumerFrame';

const market: Market = {
  id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
  precision: { amount: 0.1, price: 0.1 },
  limits: { amount: { min: 0.1, max: 1000 }, leverage: { min: 1, max: 100 } },
};

// Extend the independent legacy fixture, preserving every old fill byte.
function fillFrame(financials?: [number, number]): ArrayBuffer {
  const base = consumerFrame({ markets: [market] }, undefined, true);
  if (!financials) return base;
  const view = new DataView(base);
  let at = 16;
  while (view.getUint16(at, true) !== 11) at += 8 + view.getUint32(at + 4, true);
  const size = view.getUint32(at + 4, true);
  const end = at + 8 + size;
  const buffer = new ArrayBuffer(base.byteLength + 16);
  const bytes = new Uint8Array(buffer);
  bytes.set(new Uint8Array(base, 0, end));
  bytes.set(new Uint8Array(base, end), end + 16);
  const extended = new DataView(buffer);
  extended.setUint32(at + 4, size + 16, true);
  financials.forEach((value, i) => extended.setFloat64(end + i * 8, value, true));
  return buffer;
}

afterEach(() => vi.useRealTimers());

describe('native fill and trigger metadata at the SDK boundary', () => {
  it.each([false, true])('preserves fill identities and financial values after transfer=%s', (transfer) => {
    const original = fillFrame([-3.5, -0.25]);
    const buffer = transfer ? structuredClone(original, { transfer: [original] }) : original;
    if (transfer) expect(original.byteLength).toBe(0);
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    const frame = new Frame(buffer);
    applier.apply(frame);
    expect(applier.fills(frame)).toEqual([{
      id: 'order-1', orderId: 'order-1', symbol: 'BTCUSDT', side: OrderSide.Buy,
      price: 100, amount: 0.3, realizedPnl: -3.5, fee: -0.25,
    }]);
  });

  it('retains financial zeroes while legacy frames leave missing values absent', () => {
    const applier = new FrameApplier(new DefaultStore());
    for (const values of [undefined, [0, 0] as [number, number], [NaN, Infinity] as [number, number]]) {
      const frame = new Frame(fillFrame(values));
      applier.apply(frame);
      const fill = applier.fills(frame)[0];
      expect(fill.amount).toBe(0.3);
      expect(fill.orderId).toBe('order-1');
      if (values?.[0] === 0) expect(fill).toMatchObject({ realizedPnl: 0, fee: 0 });
      else {
        expect(fill).not.toHaveProperty('realizedPnl');
        expect(fill).not.toHaveProperty('fee');
      }
    }
  });

  it('preserves the generic trigger alongside conditional-order fields', () => {
    const order: Order = {
      id: 'conditional', symbol: 'BTCUSDT', side: OrderSide.Sell, type: OrderType.Conditional,
      status: OrderStatus.Open, price: 90, amount: 1, remaining: 1, filled: 0,
      reduceOnly: true, lastUpdate: 1, triggerType: OrderStopTriggerType.MarkPrice,
    };
    const store = new DefaultStore();
    new FrameApplier(store).apply(new Frame(consumerFrame({ markets: [market], orders: [order] })));
    expect(store.orders[0]).toMatchObject({ type: OrderType.Conditional, triggerType: OrderStopTriggerType.MarkPrice });
  });

  it('attributes a decoded execution to the actual chaser order', async () => {
    vi.useFakeTimers();
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    const frame = new Frame(fillFrame([0, 0]));
    applier.apply(frame);
    store.update({ tickers: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', last: 100, bid: 100, ask: 101 } as never] });
    const exchange = new BaseExchange('hyperliquid', { key: '', secret: '', publicDataOnly: true }, store);
    exchange.placeOrder = vi.fn().mockResolvedValue(['order-1']);
    exchange.cancelOrders = vi.fn().mockResolvedValue(undefined);
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT', size: 2, side: OrderSide.Buy, maxAttempts: 10, delay: 5000,
    });
    const manager = new ChaserOrderManager();
    manager.add(instance);
    try {
      await vi.advanceTimersByTimeAsync(1000);
      expect(instance.currentOrderIds).toEqual(['order-1']);
      for (const fill of applier.fills(frame)) exchange.emitter.emit('fill', fill);
      expect(instance.filledSize).toBeCloseTo(0.3);
      expect(instance.remainingSize).toBe(1.7);
      expect(exchange.placeOrder).toHaveBeenCalledOnce();
    } finally {
      manager.cleanupExchange(exchange);
      exchange.dispose();
    }
  });
});
