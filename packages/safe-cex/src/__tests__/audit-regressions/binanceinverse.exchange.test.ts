import { describe, expect, it, vi } from 'vitest';

import { BinanceInverseExchange } from '../../exchanges/binance/binanceinverse.exchange';
import { BinanceInversePrivateWebsocket } from '../../exchanges/binance/binanceinverse.ws-private';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../../types';

const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new BinanceInverseExchange('binanceinverse', { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
  return { exchange, store };
};

const btcMarket = {
  id: 'BTC/USD:BTC',
  symbol: 'BTCUSD_PERP',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 100000 }, leverage: { min: 1, max: 125 } },
  contractSize: 100,
  isInverse: true,
};

const btcTicker = {
  id: 'BTC/USD:BTC',
  symbol: 'BTCUSD_PERP',
  bid: 49_900,
  ask: 50_100,
  last: 50_000,
  mark: 50_000,
  index: 50_000,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
};

describe('BinanceInverseExchange', () => {
  it('maps COIN-M perpetual markets as inverse contracts with contract size', async () => {
    const { exchange } = makeExchange();
    vi.spyOn(exchange.publicXhr, 'get').mockResolvedValue({
      data: {
        symbols: [
          {
            symbol: 'BTCUSD_PERP',
            pair: 'BTCUSD',
            contractType: 'PERPETUAL',
            contractStatus: 'TRADING',
            baseAsset: 'BTC',
            quoteAsset: 'USD',
            marginAsset: 'BTC',
            contractSize: '100',
            filters: [
              { filterType: 'PRICE_FILTER', tickSize: '0.1' },
              { filterType: 'LOT_SIZE', minQty: '1', maxQty: '100000', stepSize: '1' },
              { filterType: 'MARKET_LOT_SIZE', minQty: '1', maxQty: '100000', stepSize: '1' },
            ],
          },
        ],
      },
    } as any);
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: [{ symbol: 'BTCUSD_PERP', brackets: [{ initialLeverage: 125 }] }],
    } as any);

    await expect(exchange.fetchMarkets()).resolves.toEqual([
      expect.objectContaining({
        id: 'BTC/USD:BTC',
        symbol: 'BTCUSD_PERP',
        base: 'BTC',
        quote: 'USD',
        isInverse: true,
        contractSize: 100,
        precision: { amount: 1, price: 0.1 },
        limits: { amount: { min: 1, max: 100000 }, leverage: { min: 1, max: 125 } },
      }),
    ]);
  });

  it('converts native contract positions to base exposure and USD notional', async () => {
    const { exchange, store } = makeExchange();
    store.update({ markets: [btcMarket], tickers: [btcTicker] });
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: [
        {
          symbol: 'BTCUSD_PERP',
          positionAmt: '100',
          entryPrice: '49000',
          markPrice: '50000',
          unRealizedProfit: '0.004',
          liquidationPrice: '30000',
          leverage: '10',
          marginType: 'cross',
          isolatedMargin: '0',
          positionSide: 'BOTH',
          updateTime: 1,
        },
      ],
    } as any);

    const positions = await exchange.fetchPositions();
    expect(positions).toContainEqual(
      expect.objectContaining({
        id: 'BTCUSD_PERP',
        symbol: 'BTCUSD_PERP',
        side: PositionSide.Long,
        positionMode: PositionMode.OneWay,
        notional: 10_000,
        contracts: 0.2,
        unrealizedPnl: 0.004,
        marginType: MarginType.Cross,
      }),
    );
  });

  it('aggregates coin balances into USD totals while preserving coin balances', async () => {
    const { exchange, store } = makeExchange();
    store.update({ markets: [btcMarket], tickers: [btcTicker] });
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: {
        assets: [
          {
            asset: 'BTC',
            walletBalance: '0.5',
            availableBalance: '0.4',
            unrealizedProfit: '0.01',
            initialMargin: '0.05',
          },
        ],
      },
    } as any);

    const { balance } = await exchange.fetchPositionsAndBalance();
    expect(balance.total).toBe(25_000);
    expect(balance.free).toBe(20_000);
    expect(balance.used).toBe(2_500);
    expect(balance.upnl).toBe(500);
    expect(balance.info?.coinMargined).toBe(true);
    expect(balance.currencies?.BTC).toEqual({ total: 0.5, free: 0.4, used: 0.05, upnl: 0.01 });
    expect(balance.coins?.BTC).toEqual({ base: 'BTC', amount: 0.5, notional: 25_000 });
  });

  it('formats native contract quantities for limit and full-position stop orders', () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [btcMarket],
      positions: [
        {
          id: 'BTCUSD_PERP-short-one_way',
          symbol: 'BTCUSD_PERP',
          side: PositionSide.Short,
          entryPrice: 50_000,
          notional: 10_000,
          leverage: 10,
          unrealizedPnl: 0,
          contracts: 0.2,
          liquidationPrice: 60_000,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        },
      ],
    });

    const [limit] = exchange.formatCreateOrder({
      symbol: 'BTCUSD_PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 50_000,
      amount: 12,
      timeInForce: OrderTimeInForce.PostOnly,
    });
    expect(limit).toMatchObject({
      symbol: 'BTCUSD_PERP',
      type: 'LIMIT',
      side: 'BUY',
      quantity: '12',
      price: '50000',
      timeInForce: 'GTX',
    });

    const [stop] = exchange.formatCreateOrder({
      symbol: 'BTCUSD_PERP',
      type: OrderType.StopLoss,
      side: OrderSide.Buy,
      price: 55_000,
      amount: 0,
      close: true,
    });
    expect(stop).toMatchObject({
      symbol: 'BTCUSD_PERP',
      type: 'STOP_MARKET',
      side: 'BUY',
      stopPrice: '55000',
      closePosition: 'true',
    });
    expect(stop.quantity).toBeUndefined();
    expect(stop.reduceOnly).toBeUndefined();
  });

  it('maps private account updates through inverse contract size', () => {
    const { exchange, store } = makeExchange();
    store.update({ markets: [btcMarket], tickers: [btcTicker] });
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    const socket = new BinanceInversePrivateWebsocket(exchange);

    socket.handleAccountEvents([
      {
        a: {
          P: [
            {
              s: 'BTCUSD_PERP',
              pa: '100',
              ep: '50000',
              up: '0.002',
              ps: 'BOTH',
            },
          ],
        },
      },
    ]);

    expect(store.positions).toContainEqual(
      expect.objectContaining({
        id: 'BTCUSD_PERP',
        contracts: 0.2,
        notional: 10_000,
        unrealizedPnl: 0.002,
        info: expect.objectContaining({ contractsNative: 100 }),
      }),
    );
  });

  it('leaves ungraded history axes on base capability fallbacks', async () => {
    const { exchange } = makeExchange();

    expect(exchange.capabilities).toMatchObject({ fillHistory: false, orderHistory: false, incomeHistory: false });
    await expect(exchange.fetchHistoricalPositions({ account: 'binanceinverse-test' })).rejects.toThrow(
      'no journal records path declared',
    );
  });
});
