import type { Market } from '../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BlofinExchange } from './reference/blofin/blofin.exchange';
import { DefaultStore } from '../store/store.base';
import { ENDPOINTS } from '../exchanges/blofin/blofin.types';
import { createPositionId } from '../utils/createPositionId';
import { MarginType, OrderSide, OrderType, PositionMode, PositionSide } from '../types';

const createAPIMock = vi.fn();

vi.mock('./reference/blofin/blofin.api', () => ({
  createAPI: (...args: any[]) => createAPIMock(...args),
}));

const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new BlofinExchange(
    'blofin' as any,
    {
      key: 'k',
      secret: 's',
      passphrase: 'p',
    } as any,
    store,
  );
  return { exchange, store };
};

const seedMarkets = (store: DefaultStore, markets: Market[]) => {
  store.update({
    markets,
    loaded: { ...store.loaded, markets: true },
  });
};

const RAW_BTC_INSTRUMENT = {
  instId: 'BTC-USDT',
  baseCurrency: 'BTC',
  quoteCurrency: 'USDT',
  contractValue: '0.001',
  listTime: '1673517600000',
  expireTime: '2521900800000',
  maxLeverage: '150',
  minSize: '0.1',
  lotSize: '0.1',
  tickSize: '0.1',
  instType: 'SWAP',
  contractType: 'linear',
  maxLimitSize: '20000',
  maxMarketSize: '3500',
  state: 'live',
};

const RAW_SOL_INSTRUMENT = {
  instId: 'SOL-USDT',
  baseCurrency: 'SOL',
  quoteCurrency: 'USDT',
  contractValue: '1',
  listTime: '1673517600000',
  expireTime: '2521900800000',
  maxLeverage: '50',
  minSize: '0.01',
  lotSize: '0.01',
  tickSize: '0.001',
  instType: 'SWAP',
  contractType: 'linear',
  maxLimitSize: '100000',
  maxMarketSize: '100000',
  state: 'live',
};

const RAW_BTC_SPOT_INSTRUMENT = {
  instId: 'BTC-USDT',
  baseCurrency: 'BTC',
  quoteCurrency: 'USDT',
  contractValue: '1',
  listTime: '1673517600000',
  expireTime: '2521900800000',
  maxLeverage: '1',
  minSize: '0.00001',
  lotSize: '0.00001',
  tickSize: '0.01',
  instType: 'SPOT',
  contractType: 'linear',
  maxLimitSize: '1000',
  maxMarketSize: '1000',
  state: 'live',
};

describe('Blofin contract scaling regression', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    createAPIMock.mockReturnValue({});
  });

  it('maps market precision as native lot step and keeps contractSize', async () => {
    const { exchange } = makeExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [RAW_BTC_INSTRUMENT],
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();
    expect(markets).toHaveLength(1);
    expect(markets[0].symbol).toBe('BTCUSDT');
    expect(markets[0].precision.amount).toBe(0.1);
    expect(markets[0].contractSize).toBe(0.001);
    expect(markets[0].info.contractValue).toBe(0.001);
    expect(markets[0].limits.amount.min).toBeCloseTo(0.0001, 10);
  });

  it('keeps public orderbook amounts in native contracts for downstream display conversion', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    const callback = vi.fn();
    const publicWebsocket = exchange.publicWebsocket as any;
    publicWebsocket.ws = {
      readyState: globalThis.WebSocket?.OPEN ?? 1,
      send: vi.fn(),
    };

    const unsubscribe = exchange.listenOrderBook('BTCUSDT', callback);
    const [booksHandler] = publicWebsocket.messageHandlers['books.BTC-USDT'];
    const [books5Handler] = publicWebsocket.messageHandlers['books5.BTC-USDT'];

    booksHandler({
      action: 'snapshot',
      data: [
        {
          bids: [['76701.2', '1632']],
          asks: [['76701.3', '12550']],
          prevSeqId: '0',
          seqId: '10',
        },
      ],
    });

    expect(callback).toHaveBeenLastCalledWith({
      bids: [{ price: 76701.2, amount: 1632, total: 1632 }],
      asks: [{ price: 76701.3, amount: 12550, total: 12550 }],
    });

    booksHandler({
      action: 'update',
      data: {
        bids: [['76701.2', '2000']],
        asks: [['76701.3', '12000']],
        prevSeqId: '10',
        seqId: '11',
      },
    });

    expect(callback).toHaveBeenLastCalledWith({
      bids: [{ price: 76701.2, amount: 2000, total: 2000 }],
      asks: [{ price: 76701.3, amount: 12000, total: 12000 }],
    });

    books5Handler({
      data: {
        bids: [['76701.1', '143']],
        asks: [['76701.4', '183']],
      },
    });

    expect(callback).toHaveBeenLastCalledWith({
      bids: [{ price: 76701.1, amount: 143, total: 143 }],
      asks: [{ price: 76701.4, amount: 183, total: 183 }],
    });

    unsubscribe();
  });

  it('hides non-swap instruments until Blofin spot is supported', async () => {
    const { exchange } = makeExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [RAW_BTC_INSTRUMENT, RAW_BTC_SPOT_INSTRUMENT],
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets).toHaveLength(1);
    expect(markets[0].info.instType).toBe('SWAP');
  });

  it('keeps raw contractValue=1 symbols in native sizing', async () => {
    const { exchange, store } = makeExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [RAW_SOL_INSTRUMENT],
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();
    seedMarkets(store, markets as Market[]);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange as any, 'getPositionSide').mockResolvedValue(undefined);

    expect(markets[0].precision.amount).toBe(0.01);
    expect(markets[0].contractSize).toBe(1);
    expect(markets[0].info.contractValue).toBe(1);

    const payloads = await (exchange as any).formatNormalOrder({
      symbol: 'SOLUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 0.5,
    });

    expect(payloads).toHaveLength(1);
    expect(payloads[0].size).toBe('0.5');
  });

  it('uses native contracts directly in formatNormalOrder payloads', async () => {
    const { exchange, store } = makeExchange();
    const markets = await (async () => {
      exchange.xhr = {
        get: vi.fn().mockResolvedValue({
          data: {
            data: [RAW_BTC_INSTRUMENT],
          },
        }),
      } as any;
      return exchange.fetchMarkets();
    })();

    seedMarkets(store, markets as Market[]);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange as any, 'getPositionSide').mockResolvedValue(undefined);

    const payloads = await (exchange as any).formatNormalOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 0.2,
    });

    expect(payloads).toHaveLength(1);
    expect(payloads[0].size).toBe('0.2');
  });

  it('stores fetched position mode metadata for flat-symbol margin controls', async () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '0',
          msg: 'success',
          data: [
            {
              instId: 'BTC-USDT',
              marginMode: 'cross',
              positionSide: 'net',
              leverage: '12',
            },
          ],
        },
      }),
    } as any;

    const leverage = await exchange.fetchLeverageForSymbol('BTCUSDT');

    expect(leverage).toMatchObject({
      buyLeverage: 12,
      sellLeverage: 12,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
    });
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
    });
  });

  it('commits successful position mode changes without waiting for slow refetches', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      fetchedLeverage: {
        ARBUSDT: {
          buyLeverage: 75,
          sellLeverage: 75,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        },
      },
      positions: [
        {
          id: createPositionId('ARBUSDT', PositionSide.Long, PositionMode.OneWay),
          symbol: 'ARBUSDT',
          side: PositionSide.Long,
          entryPrice: 0,
          notional: 0,
          leverage: 75,
          liquidationPrice: 0,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
          unrealizedPnl: 0,
          contracts: 0,
          info: {},
        },
      ],
    });
    const post = vi.fn().mockResolvedValue({
      data: {
        code: '0',
        msg: 'success',
        data: { positionMode: 'long_short_mode' },
      },
    });
    const fetchPositions = vi.spyOn(exchange, 'fetchPositions');
    const getOrFetchPositionMode = vi.spyOn(exchange, 'getOrFetchPositionMode');
    exchange.xhr = { post } as any;

    await exchange.changePositionMode(true, 'ARBUSDT');

    expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_POSITION_MODE, {
      positionMode: 'long_short_mode',
    });
    expect(getOrFetchPositionMode).not.toHaveBeenCalled();
    expect(fetchPositions).not.toHaveBeenCalled();
    expect((exchange as any)._accountPositionMode).toBe(PositionMode.Hedged);
    expect(store.fetchedLeverage.ARBUSDT).toMatchObject({
      buyLeverage: 75,
      sellLeverage: 75,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
    });
    expect(store.positions[0]).toMatchObject({
      id: createPositionId('ARBUSDT', PositionSide.Long, PositionMode.Hedged),
      symbol: 'ARBUSDT',
      side: PositionSide.Long,
      positionMode: PositionMode.Hedged,
      contracts: 0,
    });
  });

  it('patches confirmed margin mode metadata before follow-up refetches settle', async () => {
    const { exchange, store } = makeExchange();
    const flatArbPosition = {
      id: createPositionId('ARBUSDT', PositionSide.Long, PositionMode.OneWay),
      symbol: 'ARBUSDT',
      side: PositionSide.Long,
      entryPrice: 0,
      notional: 0,
      leverage: 75,
      liquidationPrice: 0,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
      unrealizedPnl: 0,
      contracts: 0,
      info: {},
    };
    const openSolPosition = {
      ...flatArbPosition,
      id: createPositionId('SOLUSDT', PositionSide.Long, PositionMode.OneWay),
      symbol: 'SOLUSDT',
      contracts: 1,
    };
    store.update({
      fetchedLeverage: {
        ARBUSDT: {
          buyLeverage: 75,
          sellLeverage: 75,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        },
        SOLUSDT: {
          buyLeverage: 20,
          sellLeverage: 20,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        },
      },
      positions: [flatArbPosition, openSolPosition],
    });
    const post = vi.fn().mockResolvedValue({
      data: {
        code: '0',
        msg: 'success',
        data: { marginMode: 'isolated' },
      },
    });
    const getOrFetchMarginType = vi.spyOn(exchange, 'getOrFetchMarginType').mockImplementation(async () => {
      expect((exchange as any)._accountMarginType).toBe(MarginType.Isolated);
      expect(store.fetchedLeverage.ARBUSDT.marginType).toBe(MarginType.Isolated);
      expect(store.fetchedLeverage.SOLUSDT.marginType).toBe(MarginType.Isolated);
      expect(store.positions.find((position) => position.symbol === 'ARBUSDT')?.marginType).toBe(MarginType.Isolated);
      expect(store.positions.find((position) => position.symbol === 'SOLUSDT')?.marginType).toBe(MarginType.Cross);
      return MarginType.Isolated;
    });
    const fetchPositions = vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    exchange.xhr = { post } as any;

    await exchange.changeMarginType(true, 'ARBUSDT');

    expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_MARGIN_MODE, {
      marginMode: 'isolated',
    });
    expect(getOrFetchMarginType).toHaveBeenCalledWith(undefined, true);
    expect(fetchPositions).toHaveBeenCalledWith(['ARBUSDT']);
    expect(store.fetchedLeverage.ARBUSDT.marginType).toBe(MarginType.Isolated);
    expect(store.fetchedLeverage.SOLUSDT.marginType).toBe(MarginType.Isolated);
    expect(store.positions.find((position) => position.symbol === 'ARBUSDT')?.marginType).toBe(MarginType.Isolated);
    expect(store.positions.find((position) => position.symbol === 'SOLUSDT')?.marginType).toBe(MarginType.Cross);
  });

  it('includes attachAlgoOrders compatibility payload when TP/SL are attached to normal orders', async () => {
    const { exchange, store } = makeExchange();
    const markets = await (async () => {
      exchange.xhr = {
        get: vi.fn().mockResolvedValue({
          data: {
            data: [RAW_BTC_INSTRUMENT],
          },
        }),
      } as any;
      return exchange.fetchMarkets();
    })();

    seedMarkets(store, markets as Market[]);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange as any, 'getPositionSide').mockResolvedValue('long');

    const payloads = await (exchange as any).formatNormalOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.2,
      price: 50000,
      stopLoss: 49000,
      takeProfit: 51000,
    });

    expect(payloads).toHaveLength(1);
    expect(payloads[0].slTriggerPrice).toBe('49000');
    expect(payloads[0].tpTriggerPrice).toBe('51000');
    expect(payloads[0].attachAlgoOrders).toEqual([
      {
        slTriggerPrice: '49000',
        slOrderPrice: '-1',
        tpTriggerPrice: '51000',
        tpOrderPrice: '-1',
      },
    ]);
  });

  it('targets the net position for one-way full-position stop losses', async () => {
    const { exchange, store } = makeExchange();
    const markets = await (async () => {
      exchange.xhr = {
        get: vi.fn().mockResolvedValue({
          data: {
            data: [RAW_BTC_INSTRUMENT],
          },
        }),
      } as any;
      return exchange.fetchMarkets();
    })();

    seedMarkets(store, markets as Market[]);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);

    const payload = await (exchange as any).formatAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 0,
      price: 49_000,
    });

    expect(payload).toMatchObject({
      instId: 'BTC-USDT',
      marginMode: MarginType.Cross,
      positionSide: 'net',
      side: 'sell',
      size: '-1',
      slTriggerPrice: '49000',
      slOrderPrice: '-1',
      reduceOnly: 'true',
    });
    expect(payload).not.toHaveProperty('closeFraction');
  });

  it('targets the existing hedge leg for full-position stop losses', async () => {
    const { exchange, store } = makeExchange();
    const markets = await (async () => {
      exchange.xhr = {
        get: vi.fn().mockResolvedValue({
          data: {
            data: [RAW_BTC_INSTRUMENT],
          },
        }),
      } as any;
      return exchange.fetchMarkets();
    })();

    seedMarkets(store, markets as Market[]);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.Hedged);

    const payload = await (exchange as any).formatAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 0,
      price: 49_000,
    });

    expect(payload).toMatchObject({
      instId: 'BTC-USDT',
      marginMode: MarginType.Cross,
      positionSide: 'long',
      side: 'sell',
      size: '-1',
      slTriggerPrice: '49000',
      slOrderPrice: '-1',
      reduceOnly: 'true',
    });
    expect(payload).not.toHaveProperty('closeFraction');
  });

  it('returns an actionable error for unsupported multi-trade TP/SL placement', async () => {
    const { exchange } = makeExchange();
    const post = vi
      .fn()
      .mockResolvedValue({
        data: {
          code: '102143',
          msg: 'Please select a position when Reduce Only is enabled in Multi-Trade mode.',
        },
      });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    exchange.xhr = { post } as any;

    await expect(
      (exchange as any).placeAlgoOrderBatch([
        {
          instId: 'BTC-USDT',
          marginMode: MarginType.Cross,
          positionSide: 'long',
          side: 'sell',
          size: '-1',
          slTriggerPrice: '49000',
          slOrderPrice: '-1',
          reduceOnly: 'true',
        },
      ]),
    ).rejects.toThrow('BloFin Multi-Position mode is not supported in Tealstreet yet.');

    expect(post).toHaveBeenCalledTimes(1);
    expect(emitAndLogError).toHaveBeenCalledWith(
      'BloFin Multi-Position mode is not supported in Tealstreet yet. Turn off Multi-Position mode on BloFin, then retry from Tealstreet.',
    );
  });

  it('maps positions from native contracts to base using contractSize', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    const positions = exchange.mapPositions([
      {
        instId: 'BTC-USDT',
        positions: '200',
        positionSide: 'net',
        markPrice: '50000',
        averagePrice: '49000',
        unrealizedPnl: '10',
        leverage: '50',
        marginMode: 'cross',
        liquidationPrice: '30000',
      },
    ] as any);

    expect(positions).toHaveLength(1);
    expect(positions[0].contracts).toBeCloseTo(0.2, 10);
    expect(positions[0].notional).toBeCloseTo(10000, 6);
    expect(positions[0].extra?.contractsNative).toBe(200);
  });

  it('preserves liquidation price when Blofin sends a blank position field', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);
    store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.OneWay),
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 49000,
          notional: 10000,
          leverage: 50,
          unrealizedPnl: 0,
          contracts: 0.2,
          liquidationPrice: 30000,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
          info: {},
        },
      ],
    });

    const positions = exchange.mapPositions([
      {
        instId: 'BTC-USDT',
        positions: '200',
        positionSide: 'net',
        markPrice: '50000',
        averagePrice: '49000',
        unrealizedPnl: '10',
        leverage: '50',
        marginMode: 'cross',
        liquidationPrice: '',
      },
    ] as any);

    expect(positions[0].liquidationPrice).toBe(30000);
  });

  it('maps order remaining in base and preserves native remaining in extra', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    const orders = exchange.mapOrders([
      {
        orderId: 'o-1',
        instId: 'BTC-USDT',
        orderCategory: 'normal',
        side: 'buy',
        orderType: 'limit',
        size: '200',
        filledSize: '50',
        price: '50000',
        state: 'live',
        reduceOnly: 'false',
      },
    ]);

    expect(orders).toHaveLength(1);
    expect(orders[0].amount).toBeCloseTo(0.2, 10);
    expect(orders[0].remaining).toBeCloseTo(0.15, 10);
    expect(orders[0].extra?.remainingNative).toBe(150);
    expect(orders[0].extra?.filledInPlacementUnits).toBe(50);
  });

  it('ignores non-swap private order rows that collide with swap market ids', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    const orders = exchange.mapOrders([
      {
        orderId: 'spot-o-1',
        instId: 'BTC-USDT',
        instType: 'SPOT',
        orderCategory: 'normal',
        side: 'buy',
        orderType: 'limit',
        size: '1',
        filledSize: '0',
        price: '50000',
        state: 'live',
        reduceOnly: 'false',
      },
    ]);

    expect(orders).toHaveLength(0);
  });

  it('ignores non-swap private position rows that collide with swap market ids', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    const positions = exchange.mapPositions([
      {
        instId: 'BTC-USDT',
        instType: 'SPOT',
        positions: '1',
        positionSide: 'net',
        markPrice: '50000',
        averagePrice: '49000',
        unrealizedPnl: '10',
        leverage: '1',
        marginMode: 'cross',
        liquidationPrice: '',
      },
    ] as any);

    expect(positions).toHaveLength(0);
  });

  it('uses native contracts when moving unfilled limit order price', async () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.01,
      info: { contractValue: 0.01, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    const orders = exchange.mapOrders([
      {
        orderId: 'o-1',
        instId: 'BTC-USDT',
        orderCategory: 'normal',
        side: 'sell',
        orderType: 'limit',
        size: '101.1',
        filledSize: '0',
        price: '67000',
        state: 'live',
        reduceOnly: 'false',
      },
    ]);
    expect(orders[0].amount).toBeCloseTo(1.011, 10);
    expect(orders[0].extra?.remainingNative).toBeCloseTo(101.1, 10);

    store.update({ orders });

    vi.spyOn(exchange, 'cancelOrders').mockResolvedValue(undefined);
    const placeOrderSpy = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['o-2']);

    await exchange.updateOrder({
      order: orders[0],
      update: { price: 68000 },
    });

    expect(placeOrderSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        price: 68000,
        amount: 101.1,
      }),
    );
  });

  it('emits ws fill amount in base using contractSize', () => {
    vi.useFakeTimers();
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    const fills: Array<{ amount: number; price: number; symbol: string }> = [];
    exchange.on('fill', (fill: { amount: number; price: number; symbol: string }) => fills.push(fill));

    exchange.privateWebsocket.handleOrderTopic({
      data: [
        {
          orderId: 'o-1',
          instId: 'BTC-USDT',
          orderCategory: 'normal',
          side: 'buy',
          orderType: 'market',
          size: '200',
          filledSize: '200',
          price: '0',
          state: 'filled',
          averagePrice: '50000',
          reduceOnly: 'false',
        },
      ],
    });

    expect(fills).toHaveLength(1);
    expect(fills[0].amount).toBeCloseTo(0.2, 10);
    expect(fills[0].price).toBe(50000);
    expect(fills[0].symbol).toBe('BTCUSDT');

    vi.clearAllTimers();
    vi.useRealTimers();
  });

  // Cumulative 1 then 1.2 contracts: the second fill is 0.2 at its own price,
  // and a replayed filled frame emits nothing.
  it('reports each partial as its own execution, without float noise', () => {
    vi.useFakeTimers();
    const { exchange, store } = makeExchange();
    seedMarkets(store, [
      {
        id: 'BTC-USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.1, price: 0.1 },
        limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
        contractSize: 0.001,
        info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
      } as Market,
    ]);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    const fills: Array<{ amount: number; price: number }> = [];
    exchange.on('fill', (fill: { amount: number; price: number }) => fills.push(fill));

    const frame = (state: string, filledSize: string, averagePrice: string) => ({
      data: [
        {
          orderId: 'o-partial',
          instId: 'BTC-USDT',
          orderCategory: 'normal',
          side: 'buy',
          orderType: 'market',
          size: '1.2',
          filledSize,
          price: '0',
          state,
          averagePrice,
          reduceOnly: 'false',
        },
      ],
    });
    exchange.privateWebsocket.handleOrderTopic(frame('partially_filled', '1', '82240.200000000000000000'));
    exchange.privateWebsocket.handleOrderTopic(frame('filled', '1.2', '82240.866666666666666667'));
    exchange.privateWebsocket.handleOrderTopic(frame('filled', '1.2', '82240.866666666666666667'));

    expect(fills.map((fill) => [fill.amount, fill.price])).toEqual([
      [0.001, 82240.2],
      [0.0002, 82244.2],
    ]);

    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('does not duplicate virtual SL when both normal and algo records carry slTriggerPrice', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    // Mimic what BloFin returns after placing a limit order with inline SL:
    // the normal order (orders-pending) echoes slTriggerPrice AND the
    // auto-generated algo record (orders-tpsl-pending) carries the same SL
    // plus a tpslId. The UI should only show ONE protective stop per entry.
    const mapped = exchange.mapOrders([
      {
        orderId: 'o-1',
        instId: 'BTC-USDT',
        orderCategory: 'normal',
        side: 'buy',
        orderType: 'limit',
        size: '100',
        filledSize: '0',
        price: '50000',
        state: 'live',
        reduceOnly: 'false',
        slTriggerPrice: '48000',
        slOrderPrice: '-1',
      },
      {
        tpslId: 't-1',
        orderId: 'o-1',
        instId: 'BTC-USDT',
        side: 'buy',
        orderType: 'trigger',
        size: '100',
        state: 'live',
        slTriggerPrice: '48000',
        slOrderPrice: '-1',
      },
    ]);

    const limits = mapped.filter((m) => m.type === OrderType.Limit);
    const stops = mapped.filter((m) => m.type === OrderType.StopLoss);

    expect(limits).toHaveLength(1);
    expect(limits[0].id).toBe('o-1');
    expect(stops).toHaveLength(1);
    expect(stops[0].id).toBe('t-1__stop_loss');
    expect(stops[0].side).toBe(OrderSide.Sell);
    expect(stops[0].price).toBe(48000);
  });

  it('does not emit a duplicate main Limit when an algo record echoes orderCategory=normal', () => {
    const { exchange, store } = makeExchange();
    const btcMarket = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.1, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
      contractSize: 0.001,
      info: { contractValue: 0.001, lotSize: 0.1, minSize: 0.1 },
    } as Market;
    seedMarkets(store, [btcMarket]);

    // Defensive: if BloFin ever returns an algo record with orderCategory='normal'
    // and a reference orderId, we must not push a second main Limit for it.
    const mapped = exchange.mapOrders([
      {
        tpslId: 't-1',
        orderId: 'o-1',
        instId: 'BTC-USDT',
        orderCategory: 'normal',
        side: 'buy',
        orderType: 'trigger',
        size: '100',
        state: 'live',
        slTriggerPrice: '48000',
        slOrderPrice: '-1',
      },
    ]);

    const limits = mapped.filter((m) => m.type === OrderType.Limit);
    expect(limits).toHaveLength(0);
  });

  it('keeps inline TP/SL normal orders on batch endpoint for scaled placement speed', async () => {
    const { exchange } = makeExchange();
    const postMock = vi.fn(async (endpoint: string, payload: any) => {
      if (endpoint === ENDPOINTS.PLACE_ORDERS_BATCH) {
        return {
          data: {
            code: '0',
            msg: 'success',
            data: payload.map((entry: any, index: number) => ({
              orderId: `batch-${index + 1}-${entry.clientOrderId}`,
              msg: 'success',
            })),
          },
        };
      }
      throw new Error(`Unexpected endpoint call: ${endpoint}`);
    });
    exchange.xhr = {
      post: postMock,
    } as any;

    const payloads = [
      {
        instId: 'ARB-USDT',
        side: 'buy',
        orderType: 'post_only',
        size: '121',
        price: '0.115',
        positionSide: 'long',
        marginMode: 'cross',
        clientOrderId: 'c1',
        slTriggerPrice: '0.111375',
        slOrderPrice: '-1',
        tpTriggerPrice: '0.1162',
        tpOrderPrice: '-1',
      },
      {
        instId: 'ARB-USDT',
        side: 'buy',
        orderType: 'post_only',
        size: '121',
        price: '0.1144',
        positionSide: 'long',
        marginMode: 'cross',
        clientOrderId: 'c2',
        slTriggerPrice: '0.111375',
        slOrderPrice: '-1',
        tpTriggerPrice: '0.1155',
        tpOrderPrice: '-1',
      },
    ];

    const orderIds = await (exchange as any).placeOrderBatch(payloads);

    expect(postMock).toHaveBeenCalledTimes(1);
    expect(postMock).toHaveBeenNthCalledWith(1, ENDPOINTS.PLACE_ORDERS_BATCH, payloads);
    expect(postMock).not.toHaveBeenCalledWith(ENDPOINTS.PLACE_ORDERS, expect.anything());
    expect(orderIds).toEqual(['batch-1-c1', 'batch-2-c2']);
  });

  it('treats "Order placed" as a successful batch placement message', async () => {
    const { exchange } = makeExchange();
    const postMock = vi.fn(async (endpoint: string, payload: any) => {
      if (endpoint === ENDPOINTS.PLACE_ORDERS_BATCH) {
        return {
          data: {
            code: '0',
            msg: 'success',
            data: payload.map((entry: any, index: number) => ({
              orderId: `batch-${index + 1}-${entry.clientOrderId}`,
              msg: 'Order placed',
            })),
          },
        };
      }
      throw new Error(`Unexpected endpoint call: ${endpoint}`);
    });
    exchange.xhr = {
      post: postMock,
    } as any;

    const payloads = [
      {
        instId: 'ARB-USDT',
        side: 'buy',
        orderType: 'post_only',
        size: '121',
        price: '0.115',
        positionSide: 'long',
        marginMode: 'cross',
        clientOrderId: 'c1',
        slTriggerPrice: '0.111375',
        slOrderPrice: '-1',
        tpTriggerPrice: '0.1162',
        tpOrderPrice: '-1',
      },
      {
        instId: 'ARB-USDT',
        side: 'buy',
        orderType: 'post_only',
        size: '121',
        price: '0.1144',
        positionSide: 'long',
        marginMode: 'cross',
        clientOrderId: 'c2',
        slTriggerPrice: '0.111375',
        slOrderPrice: '-1',
        tpTriggerPrice: '0.1155',
        tpOrderPrice: '-1',
      },
    ];

    const orderIds = await (exchange as any).placeOrderBatch(payloads);

    expect(postMock).toHaveBeenCalledTimes(1);
    expect(orderIds).toEqual(['batch-1-c1', 'batch-2-c2']);
  });

  it('synthesizes fallback TP/SL virtual orders from normal order records when no algo records exist', () => {
    const { exchange, store } = makeExchange();
    const arbMarket = {
      id: 'ARB-USDT',
      symbol: 'ARBUSDT',
      base: 'ARB',
      quote: 'USDT',
      active: true,
      precision: { amount: 1, price: 0.0001 },
      limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
      contractSize: 1,
      info: { contractValue: 1, lotSize: 1, minSize: 1 },
    } as Market;
    seedMarkets(store, [arbMarket]);

    const normalOnly = exchange.mapOrders([
      {
        orderId: 'o-1',
        instId: 'ARB-USDT',
        orderCategory: 'normal',
        side: 'buy',
        orderType: 'post_only',
        size: '604',
        filledSize: '0',
        price: '0.115',
        state: 'live',
        reduceOnly: 'false',
        tpTriggerPrice: '0.1162',
        slTriggerPrice: '0.111375',
      },
    ]);
    const enriched = (exchange as any).addInlineTpSlFallbackOrders(normalOnly);
    const takeProfits = enriched.filter((order: any) => order.type === OrderType.TakeProfit);
    const stopLosses = enriched.filter((order: any) => order.type === OrderType.StopLoss);

    expect(takeProfits).toHaveLength(1);
    expect(stopLosses).toHaveLength(1);
    expect(takeProfits[0].id).toBe('o-1__take_profit');
    expect(stopLosses[0].id).toBe('o-1__stop_loss');
    expect(takeProfits[0].price).toBe(0.1162);
    expect(stopLosses[0].price).toBe(0.111375);
  });

  it('does not synthesize normal fallback TP/SL when linked algo records already exist', () => {
    const { exchange, store } = makeExchange();
    const arbMarket = {
      id: 'ARB-USDT',
      symbol: 'ARBUSDT',
      base: 'ARB',
      quote: 'USDT',
      active: true,
      precision: { amount: 1, price: 0.0001 },
      limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
      contractSize: 1,
      info: { contractValue: 1, lotSize: 1, minSize: 1 },
    } as Market;
    seedMarkets(store, [arbMarket]);

    const normalOrders = exchange.mapOrders([
      {
        orderId: 'o-1',
        instId: 'ARB-USDT',
        orderCategory: 'normal',
        side: 'buy',
        orderType: 'post_only',
        size: '604',
        filledSize: '0',
        price: '0.115',
        state: 'live',
        reduceOnly: 'false',
        tpTriggerPrice: '0.1162',
        slTriggerPrice: '0.111375',
      },
    ]);
    const algoOrders = exchange.mapOrders([
      {
        tpslId: 't-1',
        orderId: 'o-1',
        instId: 'ARB-USDT',
        side: 'buy',
        orderType: 'trigger',
        size: '604',
        state: 'live',
        tpTriggerPrice: '0.1162',
        slTriggerPrice: '0.111375',
      },
    ]);
    const enriched = (exchange as any).addInlineTpSlFallbackOrders([...normalOrders, ...algoOrders]);
    const fallbackTakeProfits = enriched.filter((order: any) => order.id === 'o-1__take_profit');
    const fallbackStopLosses = enriched.filter((order: any) => order.id === 'o-1__stop_loss');
    const algoTakeProfits = enriched.filter((order: any) => order.id === 't-1__take_profit');
    const algoStopLosses = enriched.filter((order: any) => order.id === 't-1__stop_loss');

    expect(fallbackTakeProfits).toHaveLength(0);
    expect(fallbackStopLosses).toHaveLength(0);
    expect(algoTakeProfits).toHaveLength(1);
    expect(algoStopLosses).toHaveLength(1);
  });
});
