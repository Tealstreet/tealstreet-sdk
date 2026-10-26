import type { ExchangeOptions, Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../reference/okx/okx.exchange';
import { BROKER_ID, ENDPOINTS } from '../../exchanges/okx/okx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, PositionMode } from '../../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const createMarket = (amountStep: number, maxAmount: number): Market => ({
  id: 'BTC-USDT-SWAP',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: amountStep, price: 0.1 },
  limits: {
    amount: { min: amountStep, max: maxAmount },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 1,
});

describe('OKX oversized order regression', () => {
  it.each([
    {
      name: 'integer contract sizes',
      amount: 25,
      amountStep: 1,
      maxAmount: 10,
      expectedSizes: ['10', '10', '5'],
    },
    {
      name: 'fractional contract sizes',
      amount: 2.5,
      amountStep: 0.1,
      maxAmount: 1,
      expectedSizes: ['1', '1', '0.5'],
    },
  ])('sends exact, bounded, uniquely identified lots for $name', async (testCase) => {
    const market = createMarket(testCase.amountStep, testCase.maxAmount);
    const store = new DefaultStore();
    store.update({ markets: [market] });

    const exchange = new OKXExchange('okx', options, store);
    exchange._accountPositionMode = PositionMode.OneWay;
    exchange._accountLevel = 2;

    const post = vi.fn().mockImplementation((_endpoint, payloads: Array<Record<string, unknown>>) => {
      return Promise.resolve({
        data: {
          data: payloads.map((_, index) => ({ ordId: `lot-${index}` })),
        },
      });
    });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: market.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: testCase.amount,
      price: 50_000,
    });

    expect(post).toHaveBeenCalledOnce();
    expect(post.mock.calls[0][0]).toBe(ENDPOINTS.PLACE_ORDERS);

    const payloads = post.mock.calls[0][1] as Array<Record<string, unknown>>;
    const sizes = payloads.map(({ sz }) => Number(sz));
    const clientOrderIds = payloads.map(({ clOrdId }) => String(clOrdId));

    expect(payloads.map(({ sz }) => sz)).toEqual(testCase.expectedSizes);
    expect(sizes.every((size) => size <= testCase.maxAmount)).toBe(true);
    expect(sizes.reduce((total, size) => total + size, 0)).toBe(testCase.amount);
    expect(new Set(clientOrderIds)).toHaveLength(payloads.length);
    expect(clientOrderIds.every((id) => id.startsWith(BROKER_ID) && id.length <= 32)).toBe(true);
    expect(payloads.every((payload) => !('qty' in payload))).toBe(true);
  });

  it.each([
    { name: 'NaN maximum size', maxAmount: NaN, amountStep: 1, errorField: 'maximum order size' },
    { name: 'infinite maximum size', maxAmount: Infinity, amountStep: 1, errorField: 'maximum order size' },
    { name: 'zero maximum size', maxAmount: 0, amountStep: 1, errorField: 'maximum order size' },
    { name: 'NaN amount precision', maxAmount: 10, amountStep: NaN, errorField: 'amount precision' },
    { name: 'infinite amount precision', maxAmount: 10, amountStep: Infinity, errorField: 'amount precision' },
    { name: 'zero amount precision', maxAmount: 10, amountStep: 0, errorField: 'amount precision' },
  ])('rejects $name before submitting for $errorField', async ({ maxAmount, amountStep, errorField }) => {
    const market = createMarket(amountStep, maxAmount);
    const store = new DefaultStore();
    store.update({ markets: [market] });

    const exchange = new OKXExchange('okx', options, store);
    exchange._accountPositionMode = PositionMode.OneWay;
    exchange._accountLevel = 2;
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: market.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 50_000,
      }),
    ).rejects.toThrow(errorField);

    expect(post).not.toHaveBeenCalled();
  });

  it.each([NaN, Infinity, 0, -1, 0.1])('rejects invalid adjusted amount %s before submitting', async (amount) => {
    const market = createMarket(1, 10);
    const store = new DefaultStore();
    store.update({ markets: [market] });

    const exchange = new OKXExchange('okx', options, store);
    exchange._accountPositionMode = PositionMode.OneWay;
    exchange._accountLevel = 2;
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: market.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount,
        price: 50_000,
      }),
    ).rejects.toThrow('must be finite and positive after adjustment');

    expect(post).not.toHaveBeenCalled();
  });
});
