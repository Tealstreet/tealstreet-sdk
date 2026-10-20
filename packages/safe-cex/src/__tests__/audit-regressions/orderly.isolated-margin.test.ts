import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { ENDPOINTS } from '../reference/orderly/orderly.types';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  Market,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
} from '../../types';

const MARKET: Market = {
  id: 'BTC/USDC:USDC',
  symbol: 'BTCUSDC',
  base: 'BTC',
  quote: 'USDC',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [MARKET],
    tickers: [
      {
        id: MARKET.id,
        symbol: MARKET.symbol,
        bid: 109,
        ask: 111,
        last: 110,
        mark: 110,
        index: 110,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
      },
    ],
  });
  const exchange = new OrderlyExchange(
    'woofi',
    { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
    store,
  );
  (exchange as any)._accountPositionMode = PositionMode.OneWay;
  return { exchange, store };
};

const orderlyFamily = ['woofi', 'kodiakfi', 'mode', 'salsadex', 'what', 'tealstreet-dex'] as const;

describe('audit regression: Orderly isolated-margin support', () => {
  it('maps documented REST margin, leverage, and allocated-margin fields', () => {
    const { exchange } = createExchange();

    expect(
      exchange.mapPosition({
        symbol: 'PERP_BTC_USDC',
        position_qty: -2,
        average_open_price: 100,
        mark_price: 110,
        est_liq_price: 150,
        leverage: 12,
        imr: 0.1,
        margin_mode: 'ISOLATED',
        margin: 18.5,
      }),
    ).toEqual(
      expect.objectContaining({
        id: 'BTCUSDC:isolated',
        symbol: 'BTCUSDC',
        side: PositionSide.Short,
        contracts: 2,
        leverage: 12,
        marginType: MarginType.Isolated,
        marginSize: 18.5,
      }),
    );
  });

  it('preserves the REST position margin mode through fetchPositions reconciliation', async () => {
    const { exchange } = createExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: {
            rows: [
              {
                symbol: 'PERP_BTC_USDC',
                position_qty: 1,
                average_open_price: 100,
                mark_price: 110,
                est_liq_price: 50,
                leverage: 20,
                margin_mode: 'ISOLATED',
                margin: 5.5,
              },
            ],
            total_collateral_value: 1_000,
            free_collateral: 900,
          },
        },
      }),
    } as unknown as Axios;

    await expect(exchange.fetchPositions()).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'BTCUSDC:isolated',
          marginType: MarginType.Isolated,
          leverage: 20,
          marginSize: 5.5,
        }),
      ]),
    );
  });

  it('maps camel-case private position pushes without colliding with the cross position', () => {
    const { exchange, store } = createExchange();

    exchange.privateWebsocket.handlePositionReport({
      positions: [
        {
          symbol: 'PERP_BTC_USDC',
          positionQty: 1,
          averageOpenPrice: 100,
          markPrice: 110,
          estLiqPrice: 50,
          leverage: 10,
          marginMode: 'CROSS',
          margin: 0,
        },
        {
          symbol: 'PERP_BTC_USDC',
          positionQty: -2,
          averageOpenPrice: 100,
          markPrice: 110,
          estLiqPrice: 150,
          leverage: 20,
          marginMode: 'ISOLATED',
          margin: 11,
        },
      ],
    });

    expect(store.positions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 'BTCUSDC', marginType: MarginType.Cross, side: PositionSide.Long }),
        expect.objectContaining({
          id: 'BTCUSDC:isolated',
          marginType: MarginType.Isolated,
          side: PositionSide.Short,
        }),
      ]),
    );
  });

  it('subscribes to the private position topic after authentication', () => {
    const { exchange } = createExchange();
    const sendWithLog = vi.spyOn(exchange.privateWebsocket as any, 'sendWithLog').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'ping').mockImplementation(() => {});

    exchange.privateWebsocket.subscribe();

    expect(sendWithLog).toHaveBeenCalledWith(JSON.stringify({ topic: 'position', event: 'subscribe' }));
  });

  it('sets the symbol default mode and writes leverage for that exact margin mode', async () => {
    const { exchange } = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { post } as unknown as Axios;

    await exchange.changeMarginType(true, 'BTCUSDC');
    await exchange.setLeverage('BTCUSDC', 75);

    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.MARGIN_MODE, {
      symbol: 'PERP_BTC_USDC',
      default_margin_mode: 'ISOLATED',
    });
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.LEVERAGE, {
      leverage: 50,
      symbol: 'PERP_BTC_USDC',
      margin_mode: 'ISOLATED',
    });
  });

  it('loads the documented default margin mode before later symbol writes', async () => {
    const { exchange } = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        data: {
          rows: [
            { symbol: 'PERP_BTC_USDC', default_margin_mode: 'ISOLATED' },
            { symbol: 'PERP_ETH_USDC', default_margin_mode: 'UNKNOWN' },
          ],
        },
      },
    });
    const post = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { get, post } as unknown as Axios;

    await exchange.fetchAndUpdateMarginTypes();
    await exchange.setLeverage('BTCUSDC', 20);

    expect(get).toHaveBeenCalledWith(ENDPOINTS.MARGIN_MODES);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.LEVERAGE, {
      leverage: 20,
      symbol: 'PERP_BTC_USDC',
      margin_mode: 'ISOLATED',
    });
  });

  it('includes the selected margin mode on regular order placement', async () => {
    const { exchange } = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { success: true } })
      .mockResolvedValueOnce({ data: { data: { order_id: 42 } } });
    exchange.xhr = { post } as unknown as Axios;

    await exchange.changeMarginType(true, 'BTCUSDC');
    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
      }),
    ).resolves.toEqual(['42']);

    expect(post.mock.calls[1][1]).toEqual(
      expect.objectContaining({ symbol: 'PERP_BTC_USDC', margin_mode: 'ISOLATED' }),
    );
  });

  it('keeps positional TP/SL sibling handling scoped to the selected margin mode', async () => {
    const { exchange, store } = createExchange();
    store.update({
      orders: [
        {
          id: '11',
          parentId: '10',
          status: OrderStatus.Open,
          symbol: 'BTCUSDC',
          type: OrderType.TakeProfit,
          side: OrderSide.Sell,
          price: 120,
          amount: 1,
          reduceOnly: true,
          filled: 0,
          remaining: 1,
          lastUpdate: 1,
          info: { margin_mode: 'CROSS' },
        },
      ],
    });
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { success: true } })
      .mockResolvedValueOnce({ data: { data: { rows: [{ orderId: 99 }] } } });
    exchange.xhr = { post } as unknown as Axios;
    const cancelAlgoOrder = vi.spyOn(exchange, 'cancelAlgoOrder');

    await exchange.changeMarginType(true, 'BTCUSDC');
    await expect(
      exchange.placeAlgoOrder({
        symbol: 'BTCUSDC',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 1,
        price: 90,
      }),
    ).resolves.toEqual(['99']);

    expect(cancelAlgoOrder).not.toHaveBeenCalled();
    expect(post.mock.calls[1][1]).toEqual(
      expect.objectContaining({ symbol: 'PERP_BTC_USDC', margin_mode: 'ISOLATED' }),
    );
  });

  it('fails a reduce-only write before REST when both margin modes are ambiguous', async () => {
    const { exchange, store } = createExchange();
    const position = (id: string, marginType: MarginType): Position => ({
      id,
      symbol: 'BTCUSDC',
      side: PositionSide.Long,
      entryPrice: 100,
      notional: 110,
      leverage: 10,
      marginType,
      positionMode: PositionMode.OneWay,
      unrealizedPnl: 10,
      contracts: 1,
      liquidationPrice: 50,
    });
    store.update({
      positions: [position('BTCUSDC', MarginType.Cross), position('BTCUSDC:isolated', MarginType.Isolated)],
    });
    const post = vi.fn();
    exchange.xhr = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDC',
        type: OrderType.Market,
        side: OrderSide.Sell,
        amount: 1,
        reduceOnly: true,
      }),
    ).rejects.toThrow('ambiguous across Cross and Isolated positions');
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects non-finite leverage before REST', async () => {
    const { exchange } = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.setLeverage('BTCUSDC', Number.NaN)).rejects.toThrow('must be a finite number');
    await expect(exchange.setAllLeverage(Number.POSITIVE_INFINITY)).rejects.toThrow('must be a finite number');
    expect(post).not.toHaveBeenCalled();
  });

  it('exposes both margin modes for every Orderly-family integration', () => {
    for (const exchangeName of orderlyFamily) {
      expect(exchangeMetadata[exchangeName].marginTypes).toEqual([MarginType.Cross, MarginType.Isolated]);
      expect(exchangeMetadata[exchangeName].hideMarginTypeSelector).toBe(false);
      expect(exchangeMetadata[exchangeName].marginTypeIsGlobal).toBe(false);
    }
  });
});
