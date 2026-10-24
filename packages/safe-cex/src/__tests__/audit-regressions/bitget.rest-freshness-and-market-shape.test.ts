import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';
// The fallback-route assertion is about Tealstreet's routing, which only exists once registered.
import '../../transport/registerTealstreetTransport';

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

const rawAlgoOrder = (overrides: Record<string, any> = {}) => ({
  orderId: 'stop-1',
  symbol: 'BTCUSDT',
  planType: 'profit_plan',
  status: 'not_trigger',
  side: 'sell',
  triggerPrice: '51000',
  size: '1',
  actualSize: '0',
  posMode: 'one_way_mode',
  triggerType: 'mark_price',
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

  it('keeps and updates Classic TP/SL rows from not_trigger algo WS frames', async () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket as any;
    vi.spyOn(websocket, 'throttledFetchOrders').mockImplementation(() => {});

    websocket.handleAlgoOrdersTopic({
      data: [rawAlgoOrder({ orderId: 'tp-1', triggerPrice: '51000', status: 'not_trigger' })],
    });

    expect(exchange.store.orders).toEqual([
      expect.objectContaining({
        id: 'tp-1',
        status: OrderStatus.Open,
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        price: 51000,
      }),
    ]);

    websocket.handleAlgoOrdersTopic({
      data: [rawAlgoOrder({ orderId: 'tp-1', triggerPrice: '52000', planStatus: 'not_trigger', status: undefined })],
    });

    expect(exchange.store.orders).toEqual([
      expect.objectContaining({
        id: 'tp-1',
        status: OrderStatus.Open,
        price: 52000,
      }),
    ]);
  });

  it('routes Classic ordersAlgo WS channel aliases to the algo-order handler', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket as any;
    vi.spyOn(websocket, 'throttledFetchOrders').mockImplementation(() => {});

    websocket.onMessage({
      data: JSON.stringify({
        action: 'update',
        arg: { instType: 'USDT-FUTURES', channel: 'ordersAlgo', instId: 'default' },
        data: [rawAlgoOrder({ orderId: 'tp-1', triggerPrice: '51000', status: 'not_trigger' })],
      }),
    } as MessageEvent);

    expect(exchange.store.orders).toEqual([
      expect.objectContaining({
        id: 'tp-1',
        status: OrderStatus.Open,
        type: OrderType.TakeProfit,
        price: 51000,
      }),
    ]);
  });

  it('marks Bitget private WS degraded when no private stream update follows a REST write', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      // Only a started instance arms the watchdog; the never-started web mirror
      // no longer does (private-ws-login-and-mirror fix). This is the started one.
      (exchange as any).privateStreamStarted = true;
      const warning = vi.spyOn(exchange.emitter, 'emit');
      const privateWebsocket = exchange.privateWebsocket as any;
      const reconnect = vi.spyOn(privateWebsocket, 'reconnect').mockImplementation(() => {});
      const fetchedStop = exchange.mapOrder(rawAlgoOrder({ orderId: 'tp-1', status: 'not_trigger' }))[0];
      vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
      const fetchOrders = vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([fetchedStop]);
      vi.spyOn(exchange, 'fetchBalance').mockResolvedValue({
        free: 100,
        used: 0,
        total: 100,
        upnl: 0,
        info: {},
      });
      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({ data: { code: '00000', data: { orderId: 'tp-1' } } }),
      } as any;

      await expect(
        exchange.placeAlgoOrders([
          {
            symbol: 'BTCUSDT',
            marginCoin: 'USDT',
            productType: 'usdt-futures',
            planType: 'profit_plan',
            triggerPrice: '51000',
          },
        ]),
      ).resolves.toEqual(['tp-1']);

      await vi.advanceTimersByTimeAsync(8_000);

      expect(exchange.store.socketHealth?.privateDegradedReason).toContain(
        'Bitget private WS is not receiving live order/position updates',
      );
      expect(warning).toHaveBeenCalledWith(
        'warning',
        expect.stringContaining('Bitget private WS is not receiving live order/position updates'),
        { forceToast: true },
      );
      // The watchdog no longer flips a fallback route (removed with useFallbackUrl in the
      // bitget egress change); route changes now follow 40018 / closes before login.
      expect(reconnect).toHaveBeenCalled();
      expect(fetchOrders).toHaveBeenCalled();
      expect(exchange.store.orders).toEqual([expect.objectContaining({ id: 'tp-1', status: OrderStatus.Open })]);
      exchange.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps Bitget private WS healthy when a private stream update follows a REST write', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      // Only a started instance arms the watchdog; the never-started web mirror
      // no longer does (private-ws-login-and-mirror fix). This is the started one.
      (exchange as any).privateStreamStarted = true;
      const warning = vi.spyOn(exchange.emitter, 'emit');
      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({ data: { code: '00000', data: { orderId: 'tp-1' } } }),
      } as any;

      await exchange.placeAlgoOrders([
        {
          symbol: 'BTCUSDT',
          marginCoin: 'USDT',
          productType: 'usdt-futures',
          planType: 'profit_plan',
          triggerPrice: '51000',
        },
      ]);
      await vi.advanceTimersByTimeAsync(1_000);
      exchange.markPrivateStreamMessageReceived();
      await vi.advanceTimersByTimeAsync(7_000);

      expect(exchange.store.socketHealth?.privateDegradedReason).toBeUndefined();
      expect(warning).not.toHaveBeenCalledWith(
        'warning',
        expect.stringContaining('Bitget private WS is not receiving live order/position updates'),
        { forceToast: true },
      );
      exchange.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it('refreshes positions and emits a fill on explicit algo fill statuses (triggered / filled / full_fill)', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket as any;
    const executePositionFetch = vi.spyOn(websocket, 'executePositionFetch').mockResolvedValue(undefined);
    vi.spyOn(websocket, 'executeBalanceFetch').mockResolvedValue(undefined);
    vi.spyOn(websocket, 'throttledFetchOrders').mockImplementation(() => {});
    const fillListener = vi.fn();
    exchange.on('fill', fillListener);

    exchange.store.addOrUpdateOrder(
      exchange.mapOrder(rawAlgoOrder({ orderId: 'tp-1', status: 'not_trigger', triggerPrice: '51000' }))[0],
    );

    websocket.handleAlgoOrdersTopic({
      data: [rawAlgoOrder({ orderId: 'tp-1', status: 'triggered', actualSize: '1' })],
    });

    expect(exchange.store.orders[0]).toEqual(expect.objectContaining({ id: 'tp-1', status: OrderStatus.Closed }));
    expect(executePositionFetch).toHaveBeenCalled();
    expect(fillListener).toHaveBeenCalled();
  });

  it('treats ambiguous Closed algo statuses (executed / success) as terminal only, without phantom fills', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket as any;
    const executePositionFetch = vi.spyOn(websocket, 'executePositionFetch').mockResolvedValue(undefined);
    const executeBalanceFetch = vi.spyOn(websocket, 'executeBalanceFetch').mockResolvedValue(undefined);
    vi.spyOn(websocket, 'throttledFetchOrders').mockImplementation(() => {});
    const fillListener = vi.fn();
    exchange.on('fill', fillListener);

    exchange.store.addOrUpdateOrder(
      exchange.mapOrder(rawAlgoOrder({ orderId: 'tp-1', status: 'not_trigger', triggerPrice: '51000' }))[0],
    );

    // The shared ORDER_STATUS map treats `executed` and `success` as Closed,
    // but on Bitget WS those strings often mean a successful cancel/modify ack
    // rather than a real fill. The local row should transition to Closed but
    // the exchange must not emit a phantom `fill` event or hammer positions/
    // balance REST endpoints for what is not actually a fill.
    websocket.handleAlgoOrdersTopic({
      data: [rawAlgoOrder({ orderId: 'tp-1', status: 'executed', actualSize: '1' })],
    });

    expect(exchange.store.orders[0]).toEqual(expect.objectContaining({ id: 'tp-1', status: OrderStatus.Closed }));
    expect(fillListener).not.toHaveBeenCalled();
    expect(executePositionFetch).not.toHaveBeenCalled();
    expect(executeBalanceFetch).not.toHaveBeenCalled();
  });

  it('treats ambiguous Closed order-topic statuses (success) as terminal only', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket as any;
    const executePositionFetch = vi.spyOn(websocket, 'executePositionFetch').mockResolvedValue(undefined);
    const executeBalanceFetch = vi.spyOn(websocket, 'executeBalanceFetch').mockResolvedValue(undefined);
    vi.spyOn(websocket, 'throttledFetchOrders').mockImplementation(() => {});
    const fillListener = vi.fn();
    exchange.on('fill', fillListener);

    exchange.store.addOrUpdateOrder(
      exchange.mapOrder({
        orderId: 'o-1',
        symbol: 'BTCUSDT',
        orderType: 'limit',
        status: 'live',
        side: 'buy',
        price: '50000',
        size: '1',
      })[0],
    );

    websocket.handleOrderTopic({
      data: [
        {
          orderId: 'o-1',
          symbol: 'BTCUSDT',
          orderType: 'limit',
          status: 'success',
          side: 'buy',
          price: '50000',
          size: '1',
          baseVolume: '1',
        },
      ],
    });

    expect(exchange.store.orders[0]).toEqual(expect.objectContaining({ id: 'o-1', status: OrderStatus.Closed }));
    expect(fillListener).not.toHaveBeenCalled();
    expect(executePositionFetch).not.toHaveBeenCalled();
    expect(executeBalanceFetch).not.toHaveBeenCalled();
  });

  it('cancels stale sized Classic stops without recomputing plan type from flat positions', async () => {
    const exchange = createExchange();
    (exchange as any)._accountTypeDetected = true;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const post = vi.fn().mockResolvedValue({ data: { code: '00000', msg: 'success', data: null } });
    exchange.unlimitedXHR = { post } as any;

    await exchange.cancelAlgoOrders([
      {
        id: 'sl-1',
        symbol: 'NEARUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Buy,
        amount: 41,
        info: {},
      } as any,
    ]);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.CANCEL_ALGO_ORDER,
      expect.objectContaining({
        orderId: 'sl-1',
        symbol: 'NEARUSDT',
        planType: 'loss_plan',
      }),
    );
    expect(emitAndLogError).not.toHaveBeenCalledWith(
      expect.stringContaining('Sized NEARUSDT stop became a full-position stop'),
    );
    exchange.dispose();
  });

  it('does not double-count realized PnL for non-UTA REST positions with UTA-only fee fields present', () => {
    // Bitget V2 non-UTA `POSITIONS_V2` reports realized PnL as
    // `achievedProfits + totalFee - deductedFee`. `mapPosition` must not
    // additionally sum UTA-only fields (`curRealisedPnl`, `totalFunding`,
    // `openFeeTotal`, `closeFeeTotal`) — those are already baked into the
    // primary field on the non-UTA shape, and summing again inflates the
    // number. UTA payloads go through `mapUtaPosition` with its own formula.
    const exchange = createExchange();

    const position = exchange.mapPosition({
      symbol: 'BTCUSDT',
      holdSide: 'long',
      total: '1',
      openPriceAvg: '50000',
      markPrice: '50000',
      marginMode: 'crossed',
      posMode: 'one_way_mode',
      leverage: '10',
      unrealizedPL: '0',
      liquidationPrice: '45000',
      achievedProfits: '10',
      totalFee: '2',
      deductedFee: '1',
      // Legacy stray UTA-shaped fields that MUST NOT be double-counted:
      curRealisedPnl: '100',
      totalFunding: '5',
      openFeeTotal: '3',
      closeFeeTotal: '4',
    });

    expect(position.realizedPnl).toBe(11); // 10 + 2 - 1
  });

  it('normalizes Classic WS position aliases for side, mode, size, entry, and liquidation', () => {
    const exchange = createExchange();

    const position = exchange.mapPosition({
      instId: 'ETHUSDT',
      posSide: 'short',
      holdMode: 'double_hold',
      pos: '2',
      avgPx: '3200',
      marketPrice: '3190',
      marginMode: 'fixed',
      lever: '5',
      upl: '20',
      liqPx: '3400',
      uTime: '2000',
    });

    expect(position).toEqual(
      expect.objectContaining({
        id: createPositionId('ETHUSDT', PositionSide.Short, PositionMode.Hedged),
        symbol: 'ETHUSDT',
        side: PositionSide.Short,
        positionMode: PositionMode.Hedged,
        marginType: MarginType.Isolated,
        contracts: 2,
        entryPrice: 3200,
        notional: 6380,
        leverage: 5,
        unrealizedPnl: 20,
        liquidationPrice: 3400,
      }),
    );
  });
});
