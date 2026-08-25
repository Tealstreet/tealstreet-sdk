import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';
import { PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';

const createExchange = () => {
  const store = new DefaultStore();
  const exchange = new BitgetExchange(
    'bitget',
    { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
    store,
  );
  exchange.xhr = { get: vi.fn(), post: vi.fn() } as any;
  return exchange;
};

const rawPosition = (overrides: Record<string, any> = {}) => ({
  marginCoin: 'USDT',
  symbol: 'BTCUSDT',
  holdSide: 'long',
  openDelegateSize: '0',
  marginSize: '100',
  available: '1',
  locked: '0',
  total: '1',
  leverage: '10',
  achievedProfits: '0',
  openPriceAvg: '50000',
  marginMode: 'crossed',
  posMode: 'one_way_mode',
  unrealizedPL: '0',
  liquidationPrice: '45000',
  keepMarginRate: '0.004',
  markPrice: '50000',
  breakEvenPrice: '50000',
  totalFee: '0',
  deductedFee: '0',
  takeProfit: '',
  stopLoss: '',
  takeProfitId: '',
  stopLossId: '',
  marginRatio: '0',
  assetMode: 'single',
  cTime: '1000',
  uTime: '1000',
  ...overrides,
});

describe('audit regression: Bitget REST freshness and market shape', () => {
  it('uses current contract max quantity fields before legacy maxTradeNum', async () => {
    const exchange = createExchange();
    (exchange.xhr.get as any).mockResolvedValue({
      data: {
        data: [
          {
            symbol: 'BTCUSDT',
            baseCoin: 'BTC',
            quoteCoin: 'USDT',
            symbolStatus: 'normal',
            sizeMultiplier: '0.001',
            pricePlace: '1',
            priceEndStep: '5',
            minTradeNum: '0.001',
            maxOrderQty: '123',
            maxMarketOrderQty: '45',
            maxTradeNum: '999',
            minLever: '1',
            maxLever: '125',
          },
        ],
      },
    });

    const [market] = await exchange.fetchMarkets();

    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.MARKETS, {
      params: { productType: 'usdt-futures' },
    });
    expect(market.limits.amount.max).toBe(123);
  });

  it('sends startTime whenever an OHLCV from timestamp is requested', async () => {
    const exchange = createExchange();
    const from = 1_700_000_000_000;
    const to = from + 60_000;
    (exchange.xhr.get as any).mockResolvedValue({ data: { data: [] } });

    await exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m', from, to, limit: 10 });

    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.KLINE, {
      params: {
        symbol: 'BTCUSDT',
        granularity: '1m',
        endTime: to,
        startTime: from,
        productType: 'usdt-futures',
        limit: 10,
      },
    });
  });

  it('caps paired candle start/end ranges below Bitget max window', async () => {
    const exchange = createExchange();
    const from = 1_700_000_000_000;
    const to = from + 500 * 24 * 60 * 60 * 1000;
    (exchange.xhr.get as any).mockResolvedValue({ data: { data: [] } });

    await exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1d', from, to, limit: 500 });

    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.KLINE, {
      params: expect.objectContaining({
        endTime: from + 89 * 24 * 60 * 60 * 1000,
        startTime: from,
      }),
    });
  });

  it('omits startTime for a single-candle request with equal from/to bounds', async () => {
    const exchange = createExchange();
    const at = 1_787_677_200_000;
    (exchange.xhr.get as any).mockResolvedValue({ data: { data: [] } });

    await exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1h', from: at, to: at, limit: 1 });

    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.KLINE, {
      params: {
        symbol: 'BTCUSDT',
        granularity: '1H',
        endTime: at,
        productType: 'usdt-futures',
        limit: 1,
      },
    });
  });

  it('keeps fresher WS positions when REST reconciliation returns an older row', async () => {
    const exchange = createExchange();
    const wsPosition = exchange.mapPosition(rawPosition({ total: '2', openPriceAvg: '51000', uTime: '2000' }));
    exchange.store.addOrUpdatePosition(wsPosition);
    (exchange.privateWebsocket as any).markWsPositions([wsPosition], 'update');
    exchange.fetchPositions = vi
      .fn()
      .mockResolvedValue([exchange.mapPosition(rawPosition({ total: '1', openPriceAvg: '50000', uTime: '1000' }))]);

    await (exchange.privateWebsocket as any).executePositionFetch();

    expect(exchange.store.positions[0]).toEqual(
      expect.objectContaining({
        contracts: 2,
        entryPrice: 51000,
        info: expect.objectContaining({ uTime: '2000' }),
      }),
    );
  });

  it('does not resurrect a REST position omitted by a fresh empty WS snapshot', async () => {
    const exchange = createExchange();
    const positionId = createPositionId('BTCUSDT', PositionSide.Long, PositionMode.OneWay);
    const staleRestPosition = exchange.mapPosition(rawPosition({ total: '1', uTime: '1000' }));

    exchange.store.addOrUpdatePosition(staleRestPosition);
    (exchange.privateWebsocket as any).markWsPositions([], 'snapshot');
    exchange.store.addOrUpdatePositionsFull([]);
    exchange.fetchPositions = vi.fn().mockResolvedValue([staleRestPosition]);

    await (exchange.privateWebsocket as any).executePositionFetch();

    expect(exchange.store.positions.find((position) => position.id === positionId)).toEqual(
      expect.objectContaining({
        contracts: 0,
        entryPrice: 0,
      }),
    );
  });
});
