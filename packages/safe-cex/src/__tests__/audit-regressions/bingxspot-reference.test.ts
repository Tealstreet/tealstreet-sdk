import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { historyCompletenessOf } from '../../historyCompleteness';
import { OrderSide, OrderType } from '../../types';
import { BingxSpotReference, SPOT_PATHS, SPOT_WS, spotSigningGolden } from '../reference/bingxspot/bingxspot.exchange';

const fixture = (name: string) =>
  readFileSync(resolve(process.cwd(), `../../sdk-rs/venues/bingx/tests/fixtures/spot/${name}.json`), 'utf8');
const files: Record<string, string> = {
  [SPOT_PATHS.markets]: 'markets',
  [SPOT_PATHS.ticker]: 'tickers',
  [SPOT_PATHS.balances]: 'balances',
  [SPOT_PATHS.orders]: 'orders',
  [SPOT_PATHS.fills]: 'fills',
  [SPOT_PATHS.candles]: 'candles',
  [SPOT_PATHS.depth]: 'depth',
};
const reference = () =>
  new BingxSpotReference(async (method, path, params) => {
    if (method === 'POST')
      return JSON.stringify({
        code: 0,
        data: {
          orderId: '9007199254740993',
          symbol: 'BTC-USDT',
          status: path === SPOT_PATHS.cancel ? 'CANCELED' : 'PENDING',
          side: params.side,
          type: params.type,
        },
      });
    return fixture(files[path]);
  });
describe('BingX Spot independent fixture reference', () => {
  it('maps the documented startup without derivatives or a positions read', async () => {
    const state = await reference().start();
    expect(state.markets[0]).toMatchObject({
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      isSpot: true,
      isPerpetual: false,
      precision: { amount: 0.00001, price: 0.01 },
    });
    expect(state.tickers[0]).toMatchObject({ last: 42215.15, percentage: 2.91, fundingRate: 0 });
    expect(state.positions).toEqual([]);
    expect(state.balance.total).toBeCloseTo(567017.3795652849, 6);
    expect(state.orders[0]).toMatchObject({ symbol: 'BNBUSDC', type: 'limit', side: 'sell', status: 'open' });
  });
  it('maps exact fill identities, OHLCV order and price/quantity depth levels', async () => {
    const r = reference();
    await r.fetchMarkets();
    const fills = await r.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 10 });
    expect(fills[0]).toMatchObject({ id: '36767057', qty: 0.1430254, timestamp: 1704961925 });
    expect(fills[0].orderId).toMatch(/^[0-9]+$/);
    expect((await r.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m', limit: 2 }))[0].timestamp).toBe(1702720560);
    expect((await r.fetchOrderBook('BTCUSDT')).bids[0]).toEqual([42182.22, 0.99114]);
  });
  it('uses distinct Spot order/cancel fields and base/quote sizing', async () => {
    const calls: { path: string; params: Record<string, string | number> }[] = [];
    const r = new BingxSpotReference(async (method, path, params) => {
      if (method === 'GET') return fixture(files[path]);
      calls.push({ path, params });
      return JSON.stringify({
        code: 0,
        data: {
          orderId: '9007199254740993',
          symbol: 'BTC-USDT',
          status: path === SPOT_PATHS.cancel ? 'CANCELED' : 'PENDING',
        },
      });
    });
    await r.fetchMarkets();
    expect(
      await r.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 0.002,
        price: 41000,
        clientOrderId: 'spot_1',
      }),
    ).toEqual(['9007199254740993']);
    expect(calls[0]).toMatchObject({
      path: SPOT_PATHS.order,
      params: { quoteOrderQty: 82, newClientOrderId: 'spot_1' },
    });
    expect(calls[0].params).not.toHaveProperty('quantity');
    await r.cancelOrder('9007199254740993', 'BTCUSDT');
    expect(calls[1]).toMatchObject({ path: SPOT_PATHS.cancel, params: { orderId: '9007199254740993' } });
    expect(SPOT_WS).toBe('wss://open-api-ws.bingx.com/market');
  });
  it('pins the native POST canonical bytes to a fixed independent HMAC vector', () => {
    const result = spotSigningGolden(
      {
        newClientOrderId: 'spot_1',
        price: 41000,
        quantity: 0.002,
        side: 'SELL',
        symbol: 'BTC-USDT',
        timeInForce: 'GTC',
        type: 'LIMIT',
      },
      1702720824000,
      'spot-fixture-secret',
    );
    expect(result.signature).toBe('702048d7a52c83bd2d60d2ead90eb204dcbd6b1ab658bae77d1108e94760d2fd');
    expect(JSON.parse(result.body)).toMatchObject({ quantity: 0.002, price: 41000, timestamp: 1702720824000 });
  });
  it('filters an exchange-defaulted history window and marks the gap', async () => {
    const r = reference();
    await r.fetchMarkets();
    const rows = await r.fetchHistoricalFills({ symbol: 'BTCUSDT', from: 1704961900000, to: 1704961924999, limit: 10 });
    expect(rows).toEqual([]);
    expect(historyCompletenessOf(rows)).toEqual({ incomplete: true, reason: 'coverageGap' });
  });
  it('never turns a failed/missing balance or order list into an empty snapshot', async () => {
    for (const body of ['{"code":100500,"msg":"busy"}', '{"code":0,"data":{}}']) {
      const r = new BingxSpotReference(async () => body);
      await expect(r.fetchBalance()).rejects.toThrow();
      await expect(r.fetchOrders()).rejects.toThrow();
    }
  });
});
