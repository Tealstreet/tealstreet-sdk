import { afterEach, describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../reference/woo/woo.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderType, PositionMode, PositionSide } from '../../types';

const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      applicationId: 'application-id',
      tuning: { consoleLogging: false },
    } as any,
    store,
  );
  store.update({
    markets: [
      { id: 'btc-market', symbol: 'BTCUSDT', precision: { amount: 0.001, price: 0.1 }, limits: {} } as any,
      { id: 'eth-market', symbol: 'ETHUSDT', precision: { amount: 0.001, price: 0.1 }, limits: {} } as any,
    ],
    tickers: [
      { id: 'btc-market', symbol: 'BTCUSDT', mark: 110, last: 110 } as any,
      { id: 'eth-market', symbol: 'ETHUSDT', mark: 190, last: 190 } as any,
    ],
  });
  vi.spyOn(exchange, 'getPositionMode').mockResolvedValue(PositionMode.OneWay);
  return { exchange, store };
};

let exchange: WOOXExchange | undefined;

afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  vi.restoreAllMocks();
});

describe('WOO legacy allocation-conscious list building', () => {
  it('parses known tickers in wire order and filters unknown markets', () => {
    const setup = makeExchange();
    exchange = setup.exchange;

    const tickers = exchange.parseTickers([
      { symbol: 'PERP_UNKNOWN_USDT', '24h_open': 1, '24h_close': 2 },
      {
        symbol: 'PERP_ETH_USDT',
        '24h_open': 200,
        '24h_close': 190,
        index_price: 191,
        mark_price: 190,
        last_funding_rate: 0.001,
        next_funding_time: 123,
        '24h_volume': 4,
        '24h_amount': 760,
        open_interest: 5,
      },
      {
        symbol: 'PERP_BTC_USDT',
        '24h_open': 100,
        '24h_close': 110,
        index_price: 109,
        mark_price: 110,
        last_funding_rate: 0.002,
        next_funding_time: 456,
        '24h_volume': 6,
        '24h_amount': 660,
        open_interest: 7,
      },
    ]);

    expect(tickers.map((ticker) => ticker.symbol)).toEqual(['ETHUSDT', 'BTCUSDT']);
    expect(tickers.map((ticker) => ticker.percentage)).toEqual([-5, 10]);
    expect(tickers[0]).toMatchObject({ id: 'eth-market', bid: 190, ask: 190, fundingRate: 0.001 });
    expect(tickers[1]).toMatchObject({ id: 'btc-market', bid: 110, ask: 110, openInterest: 7 });
  });

  it('retains the last known tickers and emits when the ticker collection is malformed', () => {
    const setup = makeExchange();
    exchange = setup.exchange;
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    expect(exchange.parseTickers({ rows: [] })).toBe(setup.store.tickers);
    expect(emitError).toHaveBeenCalledWith('rows.reduce is not a function');
  });

  it('maps nonzero known positions in wire order and preserves isolated-margin warnings', async () => {
    const setup = makeExchange();
    exchange = setup.exchange;
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          success: true,
          data: {
            positions: [
              { symbol: 'PERP_UNKNOWN_USDT', holding: 1, averageOpenPrice: 1 },
              { symbol: 'PERP_BTC_USDT', holding: 0, averageOpenPrice: 100 },
              {
                symbol: 'PERP_ETH_USDT',
                holding: -2,
                averageOpenPrice: 200,
                leverage: 3,
                estLiqPrice: 250,
                marginMode: 'ISOLATED',
              },
              {
                symbol: 'PERP_BTC_USDT',
                holding: 1,
                averageOpenPrice: 100,
                leverage: 5,
                estLiqPrice: 50,
                marginMode: 'CROSS',
              },
            ],
          },
        },
      }),
    } as any;

    const positions = await exchange.fetchOpenPositions();

    expect(positions.map((position) => position.symbol)).toEqual(['ETHUSDT', 'BTCUSDT']);
    expect(positions[0]).toMatchObject({
      side: PositionSide.Short,
      contracts: 2,
      unrealizedPnl: 20,
      marginType: MarginType.Isolated,
    });
    expect(positions[1]).toMatchObject({
      side: PositionSide.Long,
      contracts: 1,
      unrealizedPnl: 10,
      marginType: MarginType.Cross,
    });
    expect(emitError).toHaveBeenCalledWith('Detected isolated margin position. Only cross margin is supported');
  });

  it('maps valid limit orders in source order while skipping untracked symbols', async () => {
    const setup = makeExchange();
    exchange = setup.exchange;
    vi.spyOn(exchange, 'fetchOrdersRecursively').mockResolvedValue([
      {
        order_id: 2,
        symbol: 'PERP_BTC_USDT',
        quantity: 1,
        executed: 0.25,
        type: 'LIMIT',
        side: 'BUY',
        price: 100,
        status: 'NEW',
      },
      { order_id: 999, symbol: 'PERP_UNKNOWN_USDT', quantity: 1 },
      {
        order_id: 1,
        symbol: 'PERP_ETH_USDT',
        quantity: 2,
        executed: 0,
        type: 'POST_ONLY',
        side: 'SELL',
        price: 200,
        status: 'NEW',
      },
    ] as any);

    const orders = await exchange.fetchLimitOrders();

    expect(orders.map((order) => order.id)).toEqual(['2', '1']);
    expect(orders.map((order) => order.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
    expect(orders[0]).toMatchObject({ type: OrderType.Limit, filled: 0.25, remaining: 0.75 });
  });

  it('flattens mapped algo children in parent and child order', async () => {
    const setup = makeExchange();
    exchange = setup.exchange;
    const rows = [{ id: 'root-a' }, { id: 'root-b' }, { id: 'root-c' }];
    vi.spyOn(exchange, 'fetchOrdersRecursively').mockResolvedValue(rows as any);
    vi.spyOn(exchange, 'mapAlgoOrder').mockImplementation((row: any) => {
      if (row.id === 'root-a') return [{ id: 'a1' }, { id: 'a2' }] as any;
      if (row.id === 'root-b') return [];
      return [{ id: 'c1' }] as any;
    });

    const orders = await exchange.fetchAlgoOrders();

    expect(orders.map((order) => order.id)).toEqual(['a1', 'a2', 'c1']);
    expect(exchange.mapAlgoOrder).toHaveBeenCalledTimes(3);
  });
});
