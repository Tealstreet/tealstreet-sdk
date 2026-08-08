import type { Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it } from 'vitest';

import { BinanceExchange } from '../../exchanges/binance/binance.exchange';
import { BinancePMExchange } from '../../exchanges/binance/binancepm.exchange';
import { BinanceSpotExchange } from '../../exchanges/binance/binancespot.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const market = (maxAmount: number): Market => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: maxAmount }, leverage: { min: 1, max: 125 } },
});

const regularOrder = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
  ...overrides,
});

describe('Binance family regular-order clientOrderId', () => {
  it('uses caller clientOrderId for a single USD-M regular native order', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(10)] });
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);

    const { regular, algo } = (exchange as any).formatCreateOrder(regularOrder({ clientOrderId: 'caller-id_1' })) as {
      regular: Array<Record<string, unknown>>;
      algo: Array<Record<string, unknown>>;
    };

    expect(algo).toEqual([]);
    expect(regular).toHaveLength(1);
    expect(regular[0].newClientOrderId).toBe('caller-id_1');
  });

  it('keeps generated USD-M client IDs when no caller ID is provided', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(0.5)] });
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);

    const { regular } = (exchange as any).formatCreateOrder(regularOrder({ amount: 1.2 })) as {
      regular: Array<Record<string, unknown>>;
    };

    const ids = regular.map((payload) => String(payload.newClientOrderId));
    expect(regular.length).toBeGreaterThan(1);
    expect(new Set(ids)).toHaveLength(regular.length);
    expect(ids.every((id) => id.startsWith('x-s3vFFRQp'))).toBe(true);
  });

  it('rejects caller clientOrderId when USD-M regular order splits into multiple native orders', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(0.5)] });
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);

    expect(() =>
      (exchange as any).formatCreateOrder(regularOrder({ amount: 1.2, clientOrderId: 'caller-id_1' })),
    ).toThrow(/single native regular order/);
  });

  it('rejects caller clientOrderId on attached USD-M bracket payloads', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(10)] });
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);

    expect(() =>
      (exchange as any).formatCreateOrder(regularOrder({ clientOrderId: 'caller-id_1', stopLoss: 49_000 })),
    ).toThrow(/single native regular order/);
  });

  it('uses caller clientOrderId for a single Portfolio Margin regular native order', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(10)] });
    const exchange = new BinancePMExchange('binancepm', { key: 'key', secret: 'secret' }, store);

    const [payload] = (exchange as any).formatCreateOrder(regularOrder({ clientOrderId: 'caller-id_1' })) as Array<
      Record<string, unknown>
    >;

    expect(payload.newClientOrderId).toBe('caller-id_1');
  });

  it('rejects caller clientOrderId when Portfolio Margin regular order splits into multiple native orders', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(0.5)] });
    const exchange = new BinancePMExchange('binancepm', { key: 'key', secret: 'secret' }, store);

    expect(() =>
      (exchange as any).formatCreateOrder(regularOrder({ amount: 1.2, clientOrderId: 'caller-id_1' })),
    ).toThrow(/single native regular order/);
  });

  it('rejects caller clientOrderId on attached Portfolio Margin bracket payloads', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(10)] });
    const exchange = new BinancePMExchange('binancepm', { key: 'key', secret: 'secret' }, store);

    expect(() =>
      (exchange as any).formatCreateOrder(regularOrder({ clientOrderId: 'caller-id_1', stopLoss: 49_000 })),
    ).toThrow(/single native regular order/);
  });

  it('uses caller clientOrderId for a single Spot regular native order', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(10)] });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);

    const [payload] = (exchange as any).formatCreateOrder(regularOrder({ clientOrderId: 'spotClient1' })) as Array<
      Record<string, unknown>
    >;

    expect(payload.newClientOrderId).toBe('spotClient1');
  });

  it('rejects caller clientOrderId when Spot regular order splits into multiple native orders', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(0.5)] });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);

    expect(() =>
      (exchange as any).formatCreateOrder(regularOrder({ amount: 1.2, clientOrderId: 'spotClient1' })),
    ).toThrow(/single native regular order/);
  });

  it('rejects caller clientOrderId on attached Binance Spot bracket payloads', () => {
    const store = new DefaultStore();
    store.update({ markets: [market(10)] });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);

    expect(() =>
      (exchange as any).formatCreateOrder(regularOrder({ clientOrderId: 'spotClient1', stopLoss: 49_000 })),
    ).toThrow(/single native regular order/);
  });
});
