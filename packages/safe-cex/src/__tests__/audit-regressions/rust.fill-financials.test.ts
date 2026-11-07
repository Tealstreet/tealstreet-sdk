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

// Independent revision-1/2 fixture: no MARKETS needed because the symbol is explicit.
function identifiedFillFrame(revision: 1 | 2, financials?: [number, number], id?: string): ArrayBuffer {
  const texts = ['order-1', 'BTCUSDT', ...(id !== undefined ? [id] : [])].map((value) => new TextEncoder().encode(value));
  const stringsSize = 4 + (texts.length + 1) * 4 + texts.reduce((n, value) => n + value.length, 0);
  const fillSize = revision === 2 || financials ? 56 : 40;
  const buffer = new ArrayBuffer(16 + 8 + fillSize + 8 + stringsSize);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 2, true);
  view.setUint16(16, 11, true);
  view.setUint16(18, revision, true);
  view.setUint32(20, fillSize, true);
  const at = 24;
  [1, 0, 0xffffffff, 0].forEach((value, i) => view.setUint32(at + i * 4, value, true));
  view.setFloat64(at + 16, 100, true);
  view.setFloat64(at + 24, 0.3, true);
  view.setUint32(at + 32, 1, true);
  if (revision === 2) view.setUint32(at + 36, id === undefined ? 0xffffffff : 2, true);
  if (fillSize === 56) {
    view.setFloat64(at + 40, financials?.[0] ?? NaN, true);
    view.setFloat64(at + 48, financials?.[1] ?? NaN, true);
  }
  const strings = at + fillSize;
  view.setUint16(strings, 1, true);
  view.setUint32(strings + 4, stringsSize, true);
  view.setUint32(strings + 8, texts.length, true);
  const data = strings + 12 + (texts.length + 1) * 4;
  let offset = 0;
  texts.forEach((bytes, i) => {
    view.setUint32(strings + 12 + i * 4, offset, true);
    new Uint8Array(buffer, data + offset, bytes.length).set(bytes);
    offset += bytes.length;
  });
  view.setUint32(strings + 12 + texts.length * 4, offset, true);
  return buffer;
}

afterEach(() => vi.useRealTimers());

describe('native fill and trigger metadata at the SDK boundary', () => {
  it.each([1, 2] as const)('decodes revision %s identities with and without financials after transfer', (revision) => {
    for (const financials of [undefined, [0, -0.25] as [number, number]]) {
      for (const id of [undefined, '9007199254740997']) {
        const original = identifiedFillFrame(revision, financials, id);
        const frame = new Frame(structuredClone(original, { transfer: [original] }));
        const applier = new FrameApplier(new DefaultStore());
        applier.apply(frame);
        const fill = applier.fills(frame)[0];
        expect(fill).toMatchObject({ id: revision === 2 ? id ?? 'order-1' : 'order-1', orderId: 'order-1', symbol: 'BTCUSDT' });
        if (financials) expect(fill).toMatchObject({ realizedPnl: 0, fee: -0.25 });
        else {
          expect(fill).not.toHaveProperty('realizedPnl');
          expect(fill).not.toHaveProperty('fee');
        }
      }
    }
  });
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
    const frame = new Frame(identifiedFillFrame(2, [0, 0], 'execution-7'));
    applier.apply(new Frame(consumerFrame({ markets: [market] })));
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
