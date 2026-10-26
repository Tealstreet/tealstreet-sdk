import type { Market, OrderFillEvent } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { ExchangeEmitter } from '../../exchanges/exchangeEmitter';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { consumerFrame } from '../cutover/consumerFrame';

const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

describe('exchange event listener isolation', () => {
  const dispose: Array<() => void> = [];
  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });
  afterEach(() => {
    for (const cleanup of dispose.splice(0).reverse()) cleanup();
  });

  it('retains the TS venue store bridge after an earlier update listener throws', () => {
    const store = new DefaultStore();
    const exchange = new BinanceExchange('binance', { key: '', secret: '', publicDataOnly: true }, store);
    dispose.push(() => exchange.dispose());
    const cause = new Error('consumer failed', { cause: new Error('original cause') });
    exchange.on('update', () => {
      throw cause;
    });
    const later = vi.fn();
    exchange.on('update', later);

    expect(() => store.update({ latency: 15 })).not.toThrow();
    expect(later).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ latency: 15 }));
    expect(console.error).toHaveBeenCalledExactlyOnceWith('[binance] Event "update" listener failed', cause);
  });

  it('delivers the original binary Rust fill to a later listener before forwarding its frame', () => {
    const exchange = new RustEngineExchange('grvt', { key: '', secret: '' }, new DefaultStore());
    dispose.push(() => exchange.dispose());
    const cause = new Error('fill consumer failed');
    exchange.on('fill', () => {
      throw cause;
    });
    const received: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => received.push(fill));
    const forwarded = vi.fn((buffer: ArrayBuffer) => {
      expect(received).toHaveLength(1);
      structuredClone(buffer, { transfer: [buffer] });
    });
    exchange.onFrame = forwarded;
    const frame = consumerFrame({ markets: [market] }, undefined, true);

    (exchange as unknown as { receiveFrame(buffer: ArrayBuffer): void }).receiveFrame(frame);
    expect(received).toEqual([
      { id: 'order-1', orderId: 'order-1', symbol: 'BTCUSDT', side: 'buy', price: 100, amount: 0.3 },
    ]);
    expect(forwarded).toHaveBeenCalledTimes(1);
    expect(frame.byteLength).toBe(0);
    expect(console.error).toHaveBeenCalledExactlyOnceWith('[grvt] Event "fill" listener failed', cause);
  });

  it.each(['error', 'log'])('reports a throwing %s listener without recursively emitting errors or logs', (event) => {
    const emitter = new ExchangeEmitter('binance');
    const cause = new Error(`${event} consumer failed`);
    const first = vi.fn(() => {
      throw cause;
    });
    const later = vi.fn();
    const other = vi.fn();
    emitter.on(event, first);
    emitter.on(event, later);
    emitter.on(event === 'error' ? 'log' : 'error', other);
    const original = { message: 'venue message' };

    expect(emitter.emit(event, original, 123)).toBe(emitter);
    expect(first).toHaveBeenCalledExactlyOnceWith(original, 123);
    expect(later).toHaveBeenCalledExactlyOnceWith(original, 123);
    expect(other).not.toHaveBeenCalled();
    expect(console.error).toHaveBeenCalledExactlyOnceWith(`[binance] Event "${event}" listener failed`, cause);
  });

  it('preserves listener context, arguments, return-this and original callback removal', () => {
    const emitter = new ExchangeEmitter('binance');
    const context = { name: 'context' };
    const contexts: unknown[] = [];
    const listener = vi.fn(function (this: typeof context) {
      contexts.push(this);
    });
    const original = {};
    expect(emitter.on('event', listener, context)).toBe(emitter);
    expect(emitter.emit('event', original, 2)).toBe(emitter);
    expect(listener).toHaveBeenCalledExactlyOnceWith(original, 2);
    expect(contexts).toEqual([context]);
    expect(emitter.off('event', listener)).toBe(emitter);
    emitter.emit('event', original, 2);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(emitter.once('event', listener, context)).toBe(emitter);
    emitter.off('event', listener);
    emitter.emit('event', original, 2);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('removes a throwing once callback before a reentrant emission', () => {
    const emitter = new ExchangeEmitter('binance');
    const calls: string[] = [];
    const context = {};
    const contexts: unknown[] = [];
    const cause = new Error('once consumer failed');
    emitter.once(
      'event',
      function (this: object, value: string) {
        contexts.push(this);
        calls.push(`once:${value}`);
        emitter.emit('event', 'inner');
        throw cause;
      },
      context,
    );
    emitter.on('event', (value: string) => calls.push(`later:${value}`));

    emitter.emit('event', 'outer');
    expect(contexts).toEqual([context]);
    expect(calls).toEqual(['once:outer', 'later:inner', 'later:outer']);
    expect(console.error).toHaveBeenCalledExactlyOnceWith('[binance] Event "event" listener failed', cause);
  });

  it('keeps the current listener snapshot when callbacks add and remove subscriptions', () => {
    const emitter = new ExchangeEmitter('binance');
    const calls: string[] = [];
    const removed = () => calls.push('removed');
    const added = () => calls.push('added');
    emitter.on('event', () => {
      calls.push('first');
      emitter.off('event', removed);
      emitter.on('event', added);
    });
    emitter.on('event', removed);

    emitter.emit('event');
    expect(calls).toEqual(['first', 'removed']);
    calls.length = 0;
    emitter.emit('event');
    expect(calls).toEqual(['first', 'added']);
    emitter.off('event');
    emitter.emit('event');
    expect(calls).toEqual(['first', 'added']);
  });
});
