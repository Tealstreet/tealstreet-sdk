import { afterEach, describe, expect, it, vi } from 'vitest';

import { ENDPOINTS } from '../../exchanges/binance/binancepm.types';
import { FrameStore } from '../../store/frameStore';
import { MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../../types';
import { nativeFixture } from '../cutover/nativeFixture';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';

const malformedResponse = { code: -1003, msg: 'Too many requests' };

const createExchange = () => {
  const store = new FrameStore();
  const exchange = new BinancePMExchange('binancepm', { key: 'test-key', secret: 'test-secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};

const createPosition = () => ({
  id: 'BTCUSDT:long',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  entryPrice: 50_000,
  notional: 50_000,
  leverage: 10,
  unrealizedPnl: 0,
  contracts: 1,
  liquidationPrice: 45_000,
  positionMode: PositionMode.OneWay,
  marginType: MarginType.Cross,
});

const createOrder = () => ({
  id: 'order-1',
  symbol: 'BTCUSDT',
  status: OrderStatus.Open,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1,
  reduceOnly: false,
  filled: 0,
  lastUpdate: 1,
  remaining: 1,
});

const createMarket = () => ({
  id: 'BTC/USDT:USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
});

describe('Binance PM malformed REST response guards', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('preserves positions when position risk returns a non-array envelope', async () => {
    const { exchange, store } = createExchange();
    const preservedPositions = [createPosition()];
    store.apply(
      nativeFixture(
        { positions: preservedPositions as any },
        store.markets.map((row) => row.symbol),
      ),
    );
    vi.spyOn(exchange.papiXhr, 'get').mockResolvedValue({ data: malformedResponse } as any);

    await expect(exchange.fetchPositions()).resolves.toBe(store.positions);

    expect(exchange.emitAndLogError).toHaveBeenCalledWith(
      expect.stringContaining(`Binance PM ${ENDPOINTS.POSITION_RISK} response malformed`),
    );
  });

  it.each([
    ['regular open orders', [{ data: malformedResponse }]],
    ['conditional open orders', [{ data: [] }, { data: malformedResponse }]],
  ])('preserves orders when %s returns a non-array envelope', async (_name, responses) => {
    const { exchange, store } = createExchange();
    const preservedOrders = [createOrder()];
    store.apply(
      nativeFixture(
        { orders: preservedOrders as any },
        store.markets.map((row) => row.symbol),
      ),
    );
    const get = vi.spyOn(exchange.papiXhr, 'get');
    responses.forEach((response) => get.mockResolvedValueOnce(response as any));

    await expect(exchange.fetchOrders()).resolves.toBe(store.orders);

    expect(exchange.emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('response malformed'));
  });

  it('preserves markets when leverage brackets return a non-array envelope', async () => {
    const { exchange, store } = createExchange();
    const preservedMarkets = [createMarket()];
    store.apply(
      nativeFixture(
        { markets: preservedMarkets as any },
        store.markets.map((row) => row.symbol),
      ),
    );
    vi.spyOn(exchange.publicXhr, 'get').mockResolvedValue({
      data: {
        symbols: [
          {
            symbol: 'BTCUSDT',
            baseAsset: 'BTC',
            quoteAsset: 'USDT',
            marginAsset: 'USDT',
            contractType: 'PERPETUAL',
            status: 'TRADING',
            filters: [
              { filterType: 'PRICE_FILTER', tickSize: '0.10' },
              { filterType: 'LOT_SIZE', minQty: '0.001', maxQty: '1000', stepSize: '0.001' },
              { filterType: 'MARKET_LOT_SIZE', minQty: '0.001', maxQty: '1000', stepSize: '0.001' },
            ],
          },
        ],
      },
    } as any);
    vi.spyOn(exchange.papiXhr, 'get').mockResolvedValue({ data: malformedResponse } as any);

    await expect(exchange.fetchMarkets()).resolves.toBe(store.markets);

    expect(exchange.emitAndLogError).toHaveBeenCalledWith(
      expect.stringContaining(`Binance PM ${ENDPOINTS.LEVERAGE_BRACKET} response malformed`),
    );
  });

  it('preserves tickers when public ticker arrays return a non-array envelope', async () => {
    const { exchange, store } = createExchange();
    const preservedTickers = [{ id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', last: 50_000 }];
    store.apply(
      nativeFixture(
        { tickers: preservedTickers as any },
        store.markets.map((row) => row.symbol),
      ),
    );
    vi.spyOn(exchange.publicXhr, 'get').mockResolvedValue({ data: malformedResponse } as any);

    await expect(exchange.fetchTickers()).resolves.toBe(store.tickers);

    expect(exchange.emitAndLogError).toHaveBeenCalledWith(
      expect.stringContaining(`Binance PM ${ENDPOINTS.TICKERS_24H} response malformed`),
    );
  });
});
