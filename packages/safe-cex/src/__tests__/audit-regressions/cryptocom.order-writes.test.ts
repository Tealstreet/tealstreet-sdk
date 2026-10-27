import { afterEach, describe, expect, it, vi } from 'vitest';

import { privateProxyIpsSingapore, privateProxyIpsTokyo } from '@tealstreet/proxy-ips-lists/src';
import { createAPI, formatCryptocomApiError } from '../reference/cryptocom/cryptocom.api';
import { CryptocomExchange } from '../reference/cryptocom/cryptocom.exchange';
import { generateSignature } from '../reference/cryptocom/cryptocom.signing';
import { ENDPOINTS } from '../reference/cryptocom/cryptocom.types';
import { getExchangeMetadata } from '../../exchangeMetadata';
import { DefaultStore } from '../../store/store.base';
import {
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
} from '../../types';
import { virtualClock } from '../../utils/virtualClock';

const MARKET: Market = {
  id: 'BTCUSD-PERP',
  symbol: 'BTCUSD-PERP',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 0.001, price: 0.5 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  return new CryptocomExchange('cryptocom', { key: 'key', secret: 'secret' }, store);
};

describe('audit regression: Crypto.com order writes', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('declares explicit proxy regions for production REST routing', () => {
    const metadata = getExchangeMetadata('cryptocom');

    expect(metadata.privateProxyIps).toEqual(privateProxyIpsTokyo);
    expect(Object.keys(metadata.privateProxyRegions)).toEqual(['JA', 'SG']);
    expect(metadata.privateProxyRegions.JA).toEqual(privateProxyIpsTokyo);
    expect(metadata.privateProxyRegions.SG).toEqual(privateProxyIpsSingapore);
  });

  it('keeps proxy transport failures distinct from credential errors', () => {
    expect(
      formatCryptocomApiError({
        response: {
          status: 503,
          data: '<html><body><h1>503 Service Unavailable</h1>\nNo server is available to handle this request.\n</body></html>',
        },
      }),
    ).toBe(
      'Crypto.com proxy/transport error (503): <html><body><h1>503 Service Unavailable</h1>\nNo server is available to handle this request.\n</body></html>',
    );
  });

  it('preserves Crypto.com credential validation messages from JSON API errors', () => {
    expect(
      formatCryptocomApiError({
        response: {
          status: 401,
          data: { code: 40101, message: 'Bad API key, API secret or incorrect IP whitelist.' },
        },
      }),
    ).toBe('Crypto.com API Error 40101: Bad API key, API secret or incorrect IP whitelist.');
  });

  it('returns validation-owned auth errors without emitting exchange errors', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const api = createAPI({ key: 'key', secret: 'secret' } as any, exchange);
    api.defaults.adapter = vi.fn(async (config) =>
      Promise.reject(
        Object.assign(new Error('Request failed with status code 401'), {
          response: {
            status: 401,
            statusText: 'Unauthorized',
            data: { code: 40101, message: 'Synthetic auth failure' },
          },
          config,
          isAxiosError: true,
        }),
      ),
    );
    exchange.xhr = api;

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: 'Crypto.com API Error 40101: Synthetic auth failure',
    });
    expect(emitAndLogError).not.toHaveBeenCalled();
  });

  it('still emits auth API errors outside account validation', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const api = createAPI({ key: 'key', secret: 'secret' } as any, exchange);
    api.defaults.adapter = vi.fn(async (config) =>
      Promise.reject(
        Object.assign(new Error('Request failed with status code 401'), {
          response: {
            status: 401,
            statusText: 'Unauthorized',
            data: { code: 40101, message: 'Synthetic auth failure' },
          },
          config,
          isAxiosError: true,
        }),
      ),
    );

    await expect(api.post(ENDPOINTS.BALANCE, {})).rejects.toThrow(
      'Crypto.com API Error 40101: Synthetic auth failure',
    );
    expect(emitAndLogError).toHaveBeenCalledWith('Crypto.com API Error 40101: Synthetic auth failure');
  });

  it('re-signs a replayed private envelope over its own params, with a fresh nonce and no nesting', async () => {
    const exchange = createExchange();
    const api = createAPI({ key: 'key', secret: 'secret' } as any, exchange);
    const adapter = vi.fn(async () => ({
      data: { code: 0, result: {} },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    }));
    api.defaults.adapter = adapter as any;
    let now = 1_785_019_621_981;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => (now += 1_000));

    const params = {
      instrument_name: MARKET.id,
      side: 'BUY',
      type: 'MARKET',
      quantity: '1',
    };
    await api.post(ENDPOINTS.CREATE_ORDER, params);
    const adapterCalls = adapter.mock.calls as unknown as Array<[any]>;
    const firstRequest = adapterCalls[0]?.[0];
    expect(firstRequest).toBeDefined();
    const firstPayload = JSON.parse(firstRequest.data);

    await api.post(ENDPOINTS.CREATE_ORDER, JSON.stringify(firstPayload));
    const retryRequest = adapterCalls[1]?.[0];
    expect(retryRequest).toBeDefined();
    const retryPayload = JSON.parse(retryRequest.data);

    expect(retryPayload.params).toEqual(params);
    expect(retryPayload.nonce).toBeGreaterThan(firstPayload.nonce);
    expect(retryPayload.sig).toBe(
      generateSignature(retryPayload.method, retryPayload.id, 'key', 'secret', params, retryPayload.nonce),
    );
  });

  it('does not retry ambiguous private order writes after network errors', async () => {
    const exchange = createExchange();
    const api = createAPI({ key: 'key', secret: 'secret' } as any, exchange);
    const adapter = vi.fn(async (config) =>
      Promise.reject(Object.assign(new Error('Network Error'), { config, request: {}, isAxiosError: true })),
    );
    api.defaults.adapter = adapter as any;

    await expect(
      api.post(ENDPOINTS.CREATE_ORDER, {
        instrument_name: MARKET.id,
        side: 'BUY',
        type: 'MARKET',
        quantity: '1',
      }),
    ).rejects.toThrow('Network connection failed');
    expect(adapter).toHaveBeenCalledTimes(1);
  });

  it('hides the dead legacy margin-type switch and fails closed if called directly', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as any;

    expect(getExchangeMetadata('cryptocom').hideMarginTypeSelector).toBe(true);
    await expect(exchange.changeMarginType(true, MARKET.symbol)).rejects.toThrow('does not support switching');
    expect(post).not.toHaveBeenCalled();
  });

  it('uses the current get-accounts endpoint for stable account identity', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { this_account: { uuid: 'account-uuid' } } });
    exchange.xhr = { post } as any;

    await expect(exchange.getAccount()).resolves.toEqual({ userId: 'account-uuid', affiliateId: null });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.ACCOUNTS, {});
  });

  it('maps the safe-cex daily interval on REST candle requests', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { data: [] } });
    exchange.xhr = { post } as any;

    await exchange.fetchOHLCV({ symbol: MARKET.symbol, interval: '1d' });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.CANDLESTICK, {
      instrument_name: MARKET.symbol,
      timeframe: '1D',
    });
  });

  it.each([OrderSide.Buy, OrderSide.Sell])('sends quantity for %s MARKET orders', async (side) => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { order_id: '123' } });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({ symbol: MARKET.symbol, type: OrderType.Market, side, amount: 0.25, reduceOnly: true }),
    ).resolves.toEqual(['123']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_ORDER, {
      instrument_name: MARKET.id,
      side: side === OrderSide.Buy ? 'BUY' : 'SELL',
      type: 'MARKET',
      quantity: '0.25',
      exec_inst: ['REDUCE_ONLY'],
    });
    expect(post.mock.calls[0][1]).not.toHaveProperty('notional');
  });

  it('omits time_in_force from MARKET orders even when the UI passes a default TIF', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { order_id: '123' } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: MARKET.symbol,
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 5,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_ORDER, {
      instrument_name: MARKET.id,
      side: 'BUY',
      type: 'MARKET',
      quantity: '5',
    });
  });

  it('always sends both documented amend fields and backfills the unchanged value', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { order_id: '124' } });
    exchange.xhr = { post } as any;
    const order: Order = {
      id: '123',
      symbol: MARKET.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 60_000,
      amount: 0.25,
      filled: 0,
      remaining: 0.25,
      status: OrderStatus.Open,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      lastUpdate: 0,
    };

    await exchange.updateOrder({ order, update: { price: 60_100 } });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      order_id: '123',
      instrument_name: MARKET.symbol,
      new_quantity: '0.25',
      new_price: '60100',
    });
  });

  it.each([
    [OrderType.StopLoss, OrderStopTriggerType.LastPrice, 'STOP_LOSS', 'LAST_PRICE'],
    [OrderType.TakeProfit, OrderStopTriggerType.MarkPrice, 'TAKE_PROFIT', 'MARK_PRICE'],
  ] as const)('routes %s through advanced order management', async (type, triggerType, wireType, refPriceType) => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { order_id: 'trigger-id' } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: MARKET.symbol,
      type,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 59_000,
      reduceOnly: true,
      ...(type === OrderType.StopLoss ? { slTriggerType: triggerType } : { tpTriggerType: triggerType }),
    });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ADVANCED_CREATE_ORDER, {
      instrument_name: MARKET.id,
      side: 'SELL',
      type: wireType,
      quantity: '0.25',
      ref_price: '59000',
      ref_price_type: refPriceType,
      exec_inst: ['REDUCE_ONLY'],
    });
  });

  it('routes trigger amend and cancel through advanced endpoints', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { order_id: 'trigger-id' } });
    exchange.xhr = { post } as any;
    const order: Order = {
      id: 'trigger-id',
      symbol: MARKET.symbol,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 59_000,
      amount: 0.25,
      filled: 0,
      remaining: 0.25,
      status: OrderStatus.Open,
      reduceOnly: true,
      lastUpdate: 0,
    };

    await exchange.updateOrder({ order, update: { price: 58_500 } });
    await exchange.cancelOrder(order);

    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.ADVANCED_AMEND_ORDER, {
      order_id: 'trigger-id',
      new_quantity: '0.25',
      new_ref_price: '58500',
    });
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.ADVANCED_CANCEL_ORDER, { order_id: 'trigger-id' });
  });
});
