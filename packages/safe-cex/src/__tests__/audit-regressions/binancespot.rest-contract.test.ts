import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { BinanceSpotExchange } from '../reference/binance/binancespot.exchange';
import { ENDPOINTS } from '../../exchanges/binance/binancespot.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';

const ticker = {
  id: 'BTC/USDT',
  symbol: 'BTCUSDT',
  bid: 40_000,
  ask: 40_000,
  last: 40_000,
  mark: 40_000,
  index: 40_000,
  percentage: 0,
  fundingTime: 0,
  fundingRate: 0,
  volume: 0,
  quoteVolume: 0,
  openInterest: 0,
};

describe('Binance Spot REST contract', () => {
  it('uses the current account and current-open-orders endpoints', () => {
    expect(ENDPOINTS.ACCOUNT).toBe('/api/v3/account');
    expect(ENDPOINTS.OPEN_ORDERS).toBe('/api/v3/openOrders');
  });

  it('does not advertise full-position stop orders', () => {
    expect(exchangeMetadata.binancespot.supportsInfinityStops).toBe(false);
  });

  it('maps spot balances without expecting futures positions or totals', async () => {
    const store = new DefaultStore();
    store.update({ tickers: [ticker] });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: {
        accountType: 'SPOT',
        balances: [
          { asset: 'BTC', free: '0.10000000', locked: '0.02000000' },
          { asset: 'USDT', free: '100.00000000', locked: '10.00000000' },
        ],
      },
    } as any);

    const result = await exchange.fetchPositionsAndBalance();

    expect(result.positions).toEqual([]);
    expect(result.balance.currencies?.BTC).toMatchObject({ free: 0.1, used: 0.02 });
    expect(result.balance.total).toBeCloseTo(4910);
    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v3/account');
  });

  it('fetches all currently open spot orders without an allOrders symbol parameter', async () => {
    const store = new DefaultStore();
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] } as any);

    await expect(exchange.fetchOrders()).resolves.toEqual([]);
    expect(exchange.xhr.get).toHaveBeenCalledWith('/api/v3/openOrders');
  });

  it('uses exchange filters for tradable steps and decimal powers only as fallback', async () => {
    const store = new DefaultStore();
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: {
        symbols: [
          {
            symbol: 'BTCUSDT',
            baseAsset: 'BTC',
            quoteAsset: 'USDT',
            baseAssetPrecision: 8,
            quoteAssetPrecision: 8,
            status: 'TRADING',
            filters: [
              { filterType: 'PRICE_FILTER', tickSize: '0.10000000' },
              { filterType: 'LOT_SIZE', minQty: '0.00001000', maxQty: '9000.00000000', stepSize: '0.00001000' },
            ],
          },
          {
            symbol: 'LEGACYUSDT',
            baseAsset: 'LEGACY',
            quoteAsset: 'USDT',
            baseAssetPrecision: 6,
            quoteAssetPrecision: 4,
            status: 'TRADING',
            filters: [],
          },
        ],
      },
    } as any);

    const markets = await exchange.fetchMarkets();

    expect(markets[0].precision).toEqual({ amount: 0.00001, price: 0.1 });
    expect(markets[0].limits.amount).toEqual({ min: 0.00001, max: 9000 });
    expect(markets[1].precision.amount).toBeCloseTo(0.000001);
    expect(markets[1].precision.price).toBeCloseTo(0.0001);
  });

  it('builds spot order payloads without futures-only parameters', () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC/USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          isSpot: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 1 } },
        },
      ],
    });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);

    const payload = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.123,
      price: 40_000.1,
      reduceOnly: true,
      stopLoss: 39_000,
    });

    expect(payload[0]).toMatchObject({
      symbol: 'BTCUSDT',
      side: 'BUY',
      type: 'LIMIT',
      quantity: '0.123',
      price: '40000.1',
      timeInForce: 'GTC',
    });
    expect(payload[1]).toMatchObject({ side: 'SELL', type: 'STOP_LOSS', stopPrice: '39000' });
    for (const order of payload) {
      expect(order).not.toHaveProperty('positionSide');
      expect(order).not.toHaveProperty('closePosition');
      expect(order).not.toHaveProperty('reduceOnly');
    }
  });

  it('rejects zero-quantity spot orders before posting to Binance', () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC/USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          isSpot: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 1 } },
        },
      ],
    });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    expect(() =>
      (exchange as any).formatCreateOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        amount: 0,
        price: 39_000,
      }),
    ).toThrow('Binance Spot orders require an explicit quantity greater than zero for BTCUSDT');
    expect(emitAndLogError).toHaveBeenCalledWith(
      'Binance Spot orders require an explicit quantity greater than zero for BTCUSDT',
    );
  });

  it('does not treat a zero maxQty as an infinite split count', () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC/USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          isSpot: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 0 }, leverage: { min: 1, max: 1 } },
        },
      ],
    });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);

    const payload = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.123,
      price: 40_000,
    });

    expect(payload).toHaveLength(1);
    expect(payload[0]).toMatchObject({ quantity: '0.123', price: '40000' });
  });

  it('maps post-only to LIMIT_MAKER and rejects unsupported trailing-stop inputs', () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC/USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          isSpot: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 1 } },
        },
      ],
    });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);

    const [postOnly] = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.1,
      price: 40_000,
      timeInForce: OrderTimeInForce.PostOnly,
    });
    expect(postOnly.type).toBe('LIMIT_MAKER');
    expect(postOnly).not.toHaveProperty('timeInForce');

    expect(() =>
      (exchange as any).formatCreateOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.TrailingStopLoss,
        amount: 0.1,
        price: 39_000,
      }),
    ).toThrow(/trailingDelta/);
  });

  it('rejects failed order placement instead of resolving with no order IDs', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC/USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          isSpot: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 1 } },
        },
      ],
    });
    const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    vi.spyOn(exchange.unlimitedXHR, 'post').mockRejectedValue({
      response: { data: { msg: 'Account has insufficient balance for requested action.' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 0.1,
        price: 40_000,
      }),
    ).rejects.toMatchObject({
      response: { data: { msg: 'Account has insufficient balance for requested action.' } },
    });
  });
});
