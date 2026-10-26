import type { Axios } from 'axios';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { uuidMock } = vi.hoisted(() => ({ uuidMock: vi.fn() }));
vi.mock('../../utils/uuid', () => ({ uuid: uuidMock }));

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import type { Market, PlaceOrderOpts } from '../../types';
import { OrderSide, OrderType } from '../../types';

const MARKET: Market = {
  id: 'BTC-USD-PERP',
  symbol: 'BTC-USD-PERP',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
};

const DOGE_MARKET: Market = {
  id: 'DOGE-USD-PERP',
  symbol: 'DOGE-USD-PERP',
  base: 'DOGE',
  quote: 'USD',
  active: true,
  precision: { amount: 1, price: 0.00001 },
  limits: { amount: { min: 1, max: 1_000_000 }, leverage: { min: 1, max: 50 } },
  info: { price_bands_width: '0.1' },
};

const marketOrder = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTC-USD-PERP',
  type: OrderType.Market,
  side: OrderSide.Buy,
  amount: 0.01,
  ...overrides,
});

const createExchange = () => {
  const exchange = new ParadexExchange(
    'paradex',
    { publicDataOnly: true, tuning: { consoleLogging: false } } as any,
    new DefaultStore(),
  );
  const signOrder = vi.fn(async () => ({
    signature: '["r","s"]',
    signature_timestamp: 1_784_505_600_000,
  }));
  exchange.requestSigner = { signOrder } as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { exchange, signOrder };
};

beforeEach(() => {
  uuidMock.mockReset();
  uuidMock.mockReturnValue('singleclient1');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Paradex single-order client identity', () => {
  it('generates and signs a validated client_id and requires its exact response echo', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload.client_id).toBe('ts_singleclient1');
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).resolves.toEqual(['paradex-order-1']);

    expect(uuidMock).toHaveBeenCalledOnce();
    expect(signOrder).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledOnce();
  });

  it('keeps bare market orders on Paradex native MARKET and GTC semantics', async () => {
    const { exchange } = createExchange();
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload).toMatchObject({
        market: 'BTC-USD-PERP',
        side: 'BUY',
        type: 'MARKET',
        instruction: 'GTC',
      });
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).resolves.toEqual(['paradex-order-1']);

    expect(post).toHaveBeenCalledOnce();
  });

  it('turns slippage-bounded market orders into crossing IOC limits', async () => {
    const { exchange, signOrder } = createExchange();
    exchange.store.update({ markets: [MARKET] });
    vi.spyOn(exchange, 'fetchOrderBook').mockResolvedValue({
      bids: [{ price: 99, amount: 10, total: 990 }],
      asks: [{ price: 101, amount: 10, total: 1010 }],
    });
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload).toMatchObject({
        market: 'BTC-USD-PERP',
        side: 'BUY',
        type: 'LIMIT',
        price: '106.1',
        instruction: 'IOC',
      });
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder({ maxSlippagePct: 5 }))).resolves.toEqual(['paradex-order-1']);

    expect(signOrder).toHaveBeenCalledWith({
      market: 'BTC-USD-PERP',
      side: 'BUY',
      type: 'LIMIT',
      size: '0.01',
      price: '106.1',
    });
    expect(post).toHaveBeenCalledOnce();
  });

  it('keeps slippage-bounded sell market orders inside Paradex price bands', async () => {
    const { exchange, signOrder } = createExchange();
    exchange.store.update({
      markets: [DOGE_MARKET],
      tickers: [
        {
          id: 'DOGE-USD-PERP',
          symbol: 'DOGE-USD-PERP',
          bid: 0.087,
          ask: 0.088,
          last: 0.0875,
          mark: 0.0875,
          index: 0.0875,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        } as any,
      ],
    });
    vi.spyOn(exchange, 'fetchOrderBook').mockResolvedValue({
      bids: [{ price: 0.076, amount: 1000, total: 76 }],
      asks: [{ price: 0.088, amount: 1000, total: 88 }],
    });
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload).toMatchObject({
        market: 'DOGE-USD-PERP',
        side: 'SELL',
        type: 'LIMIT',
        price: '0.08312',
        instruction: 'IOC',
      });
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder(
        marketOrder({ symbol: 'DOGE-USD-PERP', side: OrderSide.Sell, amount: 145, maxSlippagePct: 5 }),
      ),
    ).resolves.toEqual(['paradex-order-1']);

    expect(signOrder).toHaveBeenCalledWith({
      market: 'DOGE-USD-PERP',
      side: 'SELL',
      type: 'LIMIT',
      size: '145',
      price: '0.08312',
    });
    expect(post).toHaveBeenCalledOnce();
  });

  it('rounds sell-side band clamps up so Paradex IOC prices stay inside bands', async () => {
    const { exchange, signOrder } = createExchange();
    exchange.store.update({
      markets: [DOGE_MARKET],
      tickers: [
        {
          id: 'DOGE-USD-PERP',
          symbol: 'DOGE-USD-PERP',
          bid: 0.0783,
          ask: 0.089,
          last: 0.08931,
          mark: 0.0869682,
          index: 0.0869682,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        } as any,
      ],
    });
    vi.spyOn(exchange, 'fetchOrderBook').mockResolvedValue({
      bids: [{ price: 0.0783, amount: 1000, total: 78.3 }],
      asks: [{ price: 0.089, amount: 1000, total: 89 }],
    });
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload).toMatchObject({
        market: 'DOGE-USD-PERP',
        side: 'SELL',
        type: 'LIMIT',
        price: '0.07828',
        instruction: 'IOC',
      });
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder(
        marketOrder({ symbol: 'DOGE-USD-PERP', side: OrderSide.Sell, amount: 35, maxSlippagePct: 20 }),
      ),
    ).resolves.toEqual(['paradex-order-1']);

    expect(signOrder).toHaveBeenCalledWith({
      market: 'DOGE-USD-PERP',
      side: 'SELL',
      type: 'LIMIT',
      size: '35',
      price: '0.07828',
    });
    expect(post).toHaveBeenCalledOnce();
  });

  it.each(['', 'x'.repeat(62)])('rejects invalid generated identity %j before signing or writing', async (value) => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn();
    uuidMock.mockReturnValue(value);
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).rejects.toThrow('Paradex generated an invalid client_id');

    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it.each([
    ['a missing envelope', () => undefined],
    ['a non-object envelope', () => []],
    ['an empty canonical id', (clientId: string) => ({ id: '   ', client_id: clientId })],
    ['a missing client echo', () => ({ id: 'paradex-order-1' })],
    ['a mismatched client echo', () => ({ id: 'paradex-order-1', client_id: 'ts_different' })],
  ])('fails closed on %s', async (_label, makeResponse) => {
    const { exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn(async (_endpoint: string, payload: any) => ({ data: makeResponse(payload.client_id) })),
    } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).rejects.toThrow('Paradex order response malformed');
    expect(emitAndLogError).toHaveBeenCalledOnce();
  });

  it('keeps attached child placement on its existing lifecycle after the correlated main order', async () => {
    const { exchange, signOrder } = createExchange();
    const payloads: any[] = [];
    exchange.xhr = {
      post: vi.fn(async (_endpoint: string, payload: any) => {
        payloads.push(payload);
        if (payloads.length === 1) {
          return { data: { id: 'main-order', client_id: payload.client_id } };
        }
        return { data: { id: 'take-profit-child' } };
      }),
    } as unknown as Axios;

    await expect(
      exchange.placeOrder(
        marketOrder({ type: OrderType.Limit, price: 50_000, takeProfit: 52_000 }),
      ),
    ).resolves.toEqual(['main-order', 'take-profit-child']);

    expect(signOrder).toHaveBeenCalledTimes(2);
    expect(payloads).toHaveLength(2);
    expect(payloads[0].client_id).toBe('ts_singleclient1');
    expect(payloads[1]).not.toHaveProperty('client_id');
  });
});
