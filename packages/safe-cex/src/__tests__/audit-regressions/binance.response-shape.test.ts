import { afterEach, describe, expect, it, vi } from 'vitest';

import { ENDPOINTS } from '../../exchanges/binance/binance.types';
import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from '../cutover/nativeFixture';
import { BinanceExchange } from '../reference/binance/binance.exchange';

const malformedResponse = { code: -1003, msg: 'Too many requests' };

const createExchange = () => {
  const store = new FrameStore();
  const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};

const createMarket = () => ({
  id: 'BTC/USDT:USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
});

describe('Binance malformed REST response guards', () => {
  afterEach(() => {
    vi.restoreAllMocks();
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
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: malformedResponse } as any);

    await expect(exchange.fetchMarkets()).resolves.toBe(store.markets);

    expect(exchange.emitAndLogError).toHaveBeenCalledWith(
      expect.any(Error),
      expect.stringContaining(`Binance ${ENDPOINTS.LEVERAGE_BRACKET} response malformed`),
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
      expect.any(Error),
      expect.stringContaining(`Binance ${ENDPOINTS.TICKERS_24H} response malformed`),
    );
  });
});
