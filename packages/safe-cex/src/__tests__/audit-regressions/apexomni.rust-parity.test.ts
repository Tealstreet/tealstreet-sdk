import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from '../reference/apexomni/apexomni.exchange';
import { ENDPOINTS } from '../reference/apexomni/apexomni.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionSide } from '../../types';

vi.mock('zklink-web-sdk-alpha', () => ({
  newRpcSignerWithProvider: () => ({ initZklinkSigner: vi.fn(async () => undefined) }),
}));
vi.mock('../reference/apexomni/zklink/signer/utils', () => ({ initSdk: vi.fn(async () => undefined) }));

const row = (extra: Record<string, unknown> = {}) => ({
  id: 'parent',
  symbol: 'BTC-USDT',
  side: 'BUY',
  type: 'LIMIT',
  status: 'OPEN',
  size: '2',
  price: '100',
  cumSuccessFillSize: '0',
  cumSuccessFillFee: '0',
  updatedAt: 1_700_000_000_123,
  ...extra,
});
const execution = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  orderId: 'parent',
  symbol: 'BTC-USDT',
  side: 'BUY',
  price: '102',
  size: '1',
  fee: '-0.02',
  createdAt: 1_700_000_000_123,
  ...extra,
});

const fixture = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 1 }, leverage: { min: 1, max: 100 } },
        info: { initialMarginRate: '0.01', defaultInitialMarginRate: '0.05' },
      },
    ] as any,
    tickers: [
      {
        id: 'BTC-USDT',
        symbol: 'BTCUSDT',
        bid: 100,
        ask: 100,
        last: 102,
        mark: 100,
        index: 100,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
      },
    ],
  });
  store.updateZkLinkConfig({
    id: '1',
    l2Key: 'key',
    contractAccount: { makerFeeRate: '0.001', takerFeeRate: '0.001' },
    contractConfig: { perpetualContract: [{ symbol: 'BTC-USDT', l2PairId: '1' }] },
  } as any);
  const exchange = new ApexOmniExchange(
    'apexomni' as any,
    { key: 'key', secret: 'secret', passphrase: 'passphrase', omniKeySeed: '11'.repeat(65) } as any,
    store,
  );
  vi.spyOn(exchange.privateWebsocket, 'throttleFetchOrders').mockImplementation(() => undefined);
  vi.spyOn(exchange.privateWebsocket, 'throttleFetchPositions').mockImplementation(() => undefined);
  return { exchange, store, websocket: exchange.privateWebsocket };
};

describe('ApeX Omni Rust parity corrections', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('publishes the fixed hedged account mode during authenticated startup', async () => {
    const { exchange, store } = fixture();
    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([...store.markets]);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => undefined);
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => undefined);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue({ total: 0, used: 0, free: 0, upnl: 0 });
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    await exchange.start();
    expect(store.options.isHedged).toBe(true);
  });

  it('uses the canonical position symbol so real rows replace their zero placeholders', async () => {
    const { exchange } = fixture();
    exchange.xhr = {
      get: vi.fn(async () => ({
        data: {
          data: {
            id: '1',
            positions: [{ symbol: 'BTC-USDT', side: 'LONG', size: '2', entryPrice: '100', customImr: '0.1' }],
          },
        },
      })),
    } as any;
    const positions = await exchange.fetchPositions();
    expect(positions).toHaveLength(2);
    expect(positions.find((position) => position.side === PositionSide.Long)).toMatchObject({
      id: 'BTCUSDT:long',
      contracts: 2,
    });
    expect(positions.find((position) => position.side === PositionSide.Short)).toMatchObject({
      id: 'BTCUSDT:short',
      contracts: 0,
    });
  });

  it.each([undefined, {}, { positions: null }, { positions: {} }])(
    'rejects a missing positions snapshot instead of clearing known positions: %j',
    async (data) => {
      const { exchange } = fixture();
      exchange.xhr = { get: vi.fn(async () => ({ data: { data } })) } as any;
      await expect(exchange.fetchPositions()).rejects.toThrow('valid positions snapshot');
    },
  );

  it('rejects malformed account/order rows and missing order snapshots', async () => {
    const { exchange } = fixture();
    exchange.xhr = { get: vi.fn(async () => ({ data: {} })) } as any;
    await expect(exchange.fetchOrders()).rejects.toThrow('valid orders snapshot');
    exchange.xhr = { get: vi.fn(async () => ({ data: { data: [row({ size: 'NaN' })] } })) } as any;
    await expect(exchange.fetchOrders()).rejects.toThrow('invalid quantities');
    exchange.xhr = {
      get: vi.fn(async () => ({
        data: { data: { positions: [{ symbol: 'BTC-USDT', side: 'LONG', size: 'NaN', entryPrice: '100' }] } },
      })),
    } as any;
    await expect(exchange.fetchPositions()).rejects.toThrow('invalid quantities');
  });

  it('maps the venue client ID verbatim', () => {
    const { exchange } = fixture();
    expect(exchange.mapOrder(row({ clientId: 'caller-id' }))[0].clientOrderId).toBe('caller-id');
  });

  it('maps documented WS realizedPnl while preserving explicit curRealisedPnl precedence', () => {
    const { exchange } = fixture();
    const position = { symbol: 'BTC-USDT', side: 'LONG', size: '1', entryPrice: '100', realizedPnl: '-0.010238' };
    expect(exchange.mapPosition(position)?.realizedPnl).toBe(-0.010238);
    expect(exchange.mapPosition({ ...position, curRealisedPnl: '0' })?.realizedPnl).toBe(0);
  });

  it('emits each genuine execution once and preserves equal fills with distinct IDs', () => {
    const { exchange, websocket } = fixture();
    const emit = vi.spyOn((exchange as any).emitter, 'emit');
    const rows = [execution('exec-1'), execution('exec-2')];
    websocket.handleFillTopic(rows as any);
    websocket.handleFillTopic(rows as any);
    const notifications = emit.mock.calls.filter(([event]) => event === 'fill').map(([, fill]) => fill);
    expect(notifications).toHaveLength(2);
    expect(notifications).toEqual([
      expect.objectContaining({ amount: 1, price: 102 }),
      expect.objectContaining({ amount: 1, price: 102 }),
    ]);
    expect(exchange.recentWsExecutions).toHaveLength(2);
  });

  it('caches snapshot executions without notifying and suppresses later replay', () => {
    const { exchange, websocket } = fixture();
    const emit = vi.spyOn((exchange as any).emitter, 'emit');
    websocket.handleFillTopic([execution('historic')] as any, true);
    websocket.handleFillTopic([execution('historic')] as any);
    expect(emit.mock.calls.filter(([event]) => event === 'fill')).toHaveLength(0);
    expect(exchange.recentWsExecutions).toHaveLength(1);
  });

  it('uses order cumulative totals only as cache fallback and keeps late execution quantities whole', async () => {
    const { exchange, websocket } = fixture();
    const emit = vi.spyOn((exchange as any).emitter, 'emit');
    websocket.handleOrderTopic([
      row({ status: 'FILLED', cumSuccessFillSize: '2', latestMatchFillPrice: '102' }),
    ] as any);
    websocket.handleOrderTopic([
      row({ status: 'FILLED', cumSuccessFillSize: '2', latestMatchFillPrice: '102' }),
    ] as any);
    expect(emit.mock.calls.filter(([event]) => event === 'fill')).toHaveLength(0);
    websocket.handleFillTopic([execution('late-1'), execution('late-2')] as any);
    expect(emit.mock.calls.filter(([event]) => event === 'fill').map(([, fill]) => fill)).toEqual([
      expect.objectContaining({ amount: 1 }),
      expect.objectContaining({ amount: 1 }),
    ]);
    exchange.xhr = { get: vi.fn(async () => ({ data: { data: [] } })) } as any;
    const result = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });
    expect(result).toHaveLength(2);
    expect(result.reduce((sum, fill) => sum + fill.qty, 0)).toBe(2);
  });

  it('preserves distinct REST execution IDs internally without changing execution output shape', async () => {
    const { exchange } = fixture();
    exchange.xhr = {
      get: vi.fn(async () => ({
        data: { data: { fills: [execution('trade-1'), execution('trade-2'), execution('trade-1')] } },
      })),
    } as any;
    const result = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });
    expect(result).toHaveLength(2);
    expect(result.every((fill) => !('id' in fill))).toBe(true);
  });

  it.each(['tradeId', 'tradeID', 'fillId', 'execId'])(
    'uses the explicit REST %s ahead of an aggregate id',
    async (alias) => {
      const { exchange } = fixture();
      const fills = [
        execution('order-id', { [alias]: 'trade-1' }),
        execution('order-id', { [alias]: 'trade-2' }),
        execution('order-id', { [alias]: 'trade-1' }),
      ];
      exchange.xhr = { get: vi.fn(async () => ({ data: { data: { fills } } })) } as any;
      expect(await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' })).toHaveLength(2);
    },
  );

  it('recognizes compact execution identifiers when no preferred ID is present', async () => {
    const { exchange } = fixture();
    const fills = [execution('', { id: undefined, i: 'compact-1' }), execution('', { id: undefined, i: 'compact-2' })];
    exchange.xhr = { get: vi.fn(async () => ({ data: { data: { fills } } })) } as any;
    expect(await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' })).toHaveLength(2);
  });

  it('keeps ID-less executions distinguished by their original millisecond timestamp', async () => {
    const { exchange } = fixture();
    const fills = [
      execution('', { id: undefined, createdAt: 1_700_000_000_123 }),
      execution('', { id: undefined, createdAt: 1_700_000_000_456 }),
    ];
    exchange.xhr = { get: vi.fn(async () => ({ data: { data: { fills } } })) } as any;
    const result = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });
    expect(result).toHaveLength(2);
    expect(result[0].timestamp).toBe(result[1].timestamp);
  });

  it('deduplicates history-order aggregates by native row ID while preserving distinct rows', async () => {
    const { exchange } = fixture();
    const orders = [
      execution('aggregate-1', { cumSuccessFillSize: '1' }),
      execution('aggregate-2', { cumSuccessFillSize: '1' }),
      execution('aggregate-1', { cumSuccessFillSize: '1' }),
    ];
    exchange.xhr = {
      get: vi.fn(async (path) => ({ data: { data: path === ENDPOINTS.FILLS ? [] : { orders } } })),
    } as any;
    expect(await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' })).toHaveLength(2);
  });

  it('does not let synthetic order cache entries consume the identity of a later ID-less execution', () => {
    const { exchange, websocket } = fixture();
    const emit = vi.spyOn((exchange as any).emitter, 'emit');
    websocket.handleOrderTopic([
      row({ status: 'FILLED', cumSuccessFillSize: '2', latestMatchFillPrice: '102' }),
    ] as any);
    const actual = execution('', { id: undefined, size: '2' });
    websocket.handleFillTopic([actual] as any);
    websocket.handleFillTopic([actual] as any);
    expect(emit.mock.calls.filter(([event]) => event === 'fill').map(([, fill]) => fill)).toEqual([
      expect.objectContaining({ amount: 2, price: 102 }),
    ]);
    expect(exchange.recentWsExecutions).toHaveLength(1);
  });

  it('rejects malformed realized PnL, missing sizes and invalid bracket prices atomically', async () => {
    const { exchange } = fixture();
    for (const position of [
      { symbol: 'BTC-USDT', side: 'LONG', size: '1', entryPrice: '100', curRealisedPnl: 'NaN' },
      { symbol: 'BTC-USDT', side: 'LONG', entryPrice: '100' },
      { symbol: 'BTC-USDT', side: 'UNKNOWN', size: '1', entryPrice: '100' },
    ]) {
      exchange.xhr = { get: vi.fn(async () => ({ data: { data: { positions: [position] } } })) } as any;
      await expect(exchange.fetchPositions()).rejects.toThrow('invalid');
    }
    for (const order of [row({ size: undefined }), row({ openSlParams: { triggerPrice: 'NaN' } })]) {
      exchange.xhr = { get: vi.fn(async () => ({ data: { data: [order] } })) } as any;
      await expect(exchange.fetchOrders()).rejects.toThrow('invalid');
    }
  });

  it('retains the last known balance when numeric fields are malformed', async () => {
    const { exchange, store } = fixture();
    store.update({ balance: { total: 1000, free: 900, used: 20, upnl: 4 } });
    exchange.xhr = {
      get: vi.fn(async () => ({
        data: { data: { totalEquityValue: 'NaN', initialMargin: '100', maintenanceMargin: '20' } },
      })),
    } as any;
    expect(await exchange.fetchRawBalance()).toEqual(store.balance);
  });

  it('ignores a malformed private position frame without exposing partial financial updates', () => {
    const { exchange, websocket, store } = fixture();
    const initial = exchange.mapPosition({ symbol: 'BTC-USDT', side: 'LONG', size: '1', entryPrice: '100' })!;
    store.addOrUpdatePositionsFull([initial]);
    expect(() =>
      websocket.handlePositionTopic([
        { symbol: 'BTC-USDT', side: 'LONG', size: '2', entryPrice: '100' },
        { symbol: 'BTC-USDT', side: 'SHORT', size: '1', entryPrice: '100', realizedPnl: 'NaN' },
      ] as any),
    ).not.toThrow();
    expect(store.positions).toEqual([initial]);
  });

  it('keeps terminal parent and virtual siblings from reopening in a delayed snapshot', () => {
    const { exchange, websocket, store } = fixture();
    const withLegs = row({ openSlParam: { triggerPrice: '90' }, openTpParams: { triggerPrice: '110' } });
    store.addOrUpdateOrders(exchange.mapOrder(withLegs));
    websocket.handleOrderTopic([row({ status: 'CANCELED' })] as any);
    store.addOrUpdateOrdersFull(exchange.mapOrder(withLegs));
    expect(store.orders).toHaveLength(0);
  });

  it('preserves both protective siblings and does not reuse the caller ID when amending the parent', async () => {
    const { exchange, store } = fixture();
    const orders = exchange.mapOrder(
      row({
        clientId: 'original-caller',
        openSlParam: { triggerPrice: '90', triggerPriceType: 'INDEX' },
        openTpParams: { triggerPrice: '110', triggerPriceType: 'MARKET' },
      }),
    );
    store.addOrUpdateOrders(orders);
    vi.spyOn(exchange, 'cancelOrders').mockResolvedValue(undefined);
    const place = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['replacement']);
    await exchange.updateOrder({ order: orders[0], update: { price: 101 } });
    expect(place).toHaveBeenCalledWith(
      expect.objectContaining({
        price: 101,
        stopLoss: 90,
        takeProfit: 110,
        slTriggerType: OrderStopTriggerType.IndexPrice,
        tpTriggerType: OrderStopTriggerType.LastPrice,
      }),
    );
    expect(place.mock.calls[0][0].clientOrderId).toBeUndefined();
  });

  it('splits exact whole ticks without creating size while retaining every lot within venue bounds', async () => {
    const { exchange } = fixture();
    vi.spyOn(exchange, 'signOrder').mockResolvedValue('signature');
    let counter = 0;
    const post = vi.fn(async () => ({ data: { data: { id: `order-${++counter}` } } }));
    exchange.unlimitedXHR = { post } as any;
    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 2.5,
    });
    const sizes = post.mock.calls.map((call: any[]) => Number(new URLSearchParams(call[1]).get('size')));
    expect(ids).toHaveLength(3);
    expect(sizes).toEqual([0.834, 0.833, 0.833]);
    expect(sizes.reduce((sum, size) => sum + Math.round(size * 1000), 0)).toBe(2500);
    expect(sizes.every((size) => size >= 0.001 && size <= 1)).toBe(true);
    expect(post.mock.calls.every((call: any[]) => call[0] === ENDPOINTS.CREATE_ORDER)).toBe(true);
  });
});

describe('ApeX snapshot identifiers and execution range precision', () => {
  beforeEach(() => vi.restoreAllMocks());

  it.each([undefined, null, '', '   '])('rejects missing or blank snapshot symbols: %j', async (symbol) => {
    const { exchange, store, websocket } = fixture();
    const initialPosition = exchange.mapPosition({ symbol: 'BTC-USDT', side: 'LONG', size: '2', entryPrice: '100' })!;
    store.addOrUpdatePositionsFull([initialPosition]);
    store.addOrUpdateOrders(exchange.mapOrder(row()));
    exchange.xhr = {
      get: vi.fn(async (path) => ({
        data: {
          data:
            path === ENDPOINTS.ACCOUNT
              ? { positions: [{ symbol, side: 'LONG', size: '2', entryPrice: '100' }] }
              : [row({ symbol })],
        },
      })),
    } as any;
    await expect(exchange.fetchPositions()).rejects.toThrow('invalid symbol');
    await expect(exchange.fetchOrders()).rejects.toThrow('invalid symbol or ID');
    websocket.handleOrderTopic([row({ status: 'CANCELED', symbol })] as any);
    expect(store.positions).toEqual([initialPosition]);
    expect(store.orders).toHaveLength(1);
    websocket.handleOrderTopic([row({ cumSuccessFillSize: '0.5' })] as any);
    expect(store.orders[0].filled).toBe(0.5);
  });

  it.each([undefined, null, '', '   '])(
    'rejects missing or blank order IDs before unknown-market skips: %j',
    async (id) => {
      const { exchange } = fixture();
      exchange.xhr = { get: vi.fn(async () => ({ data: { data: [row({ id, symbol: 'UNKNOWN' })] } })) } as any;
      await expect(exchange.fetchOrders()).rejects.toThrow('invalid symbol or ID');
    },
  );

  it.each([1_700_000_000_123, '1700000000.123', '2023-11-14T22:13:20.123Z', '2023-11-15T00:13:20.123+02:00'])(
    'filters raw REST time inclusively before exposing second precision: %j',
    async (createdAt) => {
      const { exchange } = fixture();
      const get = vi.fn(async () => ({
        data: {
          data: {
            fills: [
              execution('before', { createdAt: 1_700_000_000_099 }),
              execution('inside', { createdAt }),
              execution('after', { createdAt: 1_700_000_000_124 }),
            ],
          },
        },
      }));
      exchange.xhr = { get } as any;
      const result = await exchange.fetchHistoricalFills({
        symbol: 'BTCUSDT',
        from: 1_700_000_000_100,
        to: 1_700_000_000_123,
      });
      expect(result).toHaveLength(1);
      expect(result[0].timestamp).toBe(1_700_000_000);
      expect(get.mock.calls[0]).toEqual([
        ENDPOINTS.FILLS,
        expect.objectContaining({
          params: expect.objectContaining({
            beginTimeInclusive: 1_700_000_000_100,
            endTimeExclusive: 1_700_000_000_124,
          }),
        }),
      ]);
    },
  );

  it('sorts REST executions by their raw subsecond times before applying a limit', async () => {
    const { exchange } = fixture();
    exchange.xhr = {
      get: vi.fn(async () => ({
        data: {
          data: {
            fills: [
              execution('older', { createdAt: 1_700_000_000_123, orderId: 'older' }),
              execution('newer', { createdAt: '2023-11-14T22:13:20.456Z', orderId: 'newer' }),
            ],
          },
        },
      })),
    } as any;
    const result = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 1 });
    expect(result.map((fill) => fill.orderId)).toEqual(['newer']);
  });

  it.each(['fills', 'synthetic', 'consumed synthetic'] as const)(
    'keeps original milliseconds through %s cache filtering, sorting and limiting',
    async (source) => {
      const { exchange, websocket } = fixture();
      for (const millis of [456, 457, 123]) {
        if (source === 'fills') {
          websocket.handleFillTopic([
            execution('', { id: undefined, orderId: `order-${millis}`, createdAt: 1_700_000_000_000 + millis }),
          ] as any);
        } else {
          websocket.handleOrderTopic([
            row({ id: `order-${millis}`, updatedAt: 1_700_000_000_000 + millis, cumSuccessFillSize: '2' }),
          ] as any);
        }
      }
      if (source === 'consumed synthetic') exchange.consumeSyntheticExecutions('order-456', 0.5);
      exchange.xhr = { get: vi.fn(async () => ({ data: { data: [] } })) } as any;
      const result = await exchange.fetchHistoricalFills({
        symbol: 'BTCUSDT',
        from: 1_700_000_000_100,
        to: 1_700_000_000_456,
        limit: 1,
      });
      expect(result).toHaveLength(1);
      expect(result[0]).toMatchObject({
        orderId: 'order-456',
        timestamp: 1_700_000_000,
        qty: source === 'fills' ? 1 : source === 'synthetic' ? 2 : 1.5,
      });
    },
  );
});
