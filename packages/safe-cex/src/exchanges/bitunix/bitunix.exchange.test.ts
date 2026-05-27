// NOTE: These tests are excluded from vitest runs by the blanket
// `**/src/exchanges/**` pattern in packages/safe-cex/vitest.config.ts
// (added 2026-01-09, commit 51f417270e). They have never been executed.
// The tests use mocks and don't require live API connections, so they
// could run if the exclude pattern were narrowed or a separate config added.

import { afterEach, describe, expect, it, vi } from 'vitest';

import { MarginType, OrderSide, OrderType, Position, PositionMode, PositionSide } from '../../types';
import { BitunixExchange } from './bitunix.exchange';

// ---------------------------------------------------------------------------
// Minimal mock of BitunixExchange
// We import the real class but stub out network/websocket dependencies so we
// can exercise fetchPositions, formatCreateOrder, formatTPSLOrder, placeOrder,
// and placeOrderBatch in isolation.
// ---------------------------------------------------------------------------

vi.mock('axios-rate-limit', () => ({ default: (x: any) => x }));
vi.mock('./bitunix.api', () => ({ createAPI: () => ({}) }));
vi.mock('./bitunix.ws-futures', () => ({ BitunixFuturesWebsocket: vi.fn() }));
vi.mock('./bitunix.ws-private', () => ({ BitunixPrivateWebsocket: vi.fn() }));
vi.mock('./bitunix.ws-public', () => ({ BitunixPublicWebsocket: vi.fn() }));
vi.mock('../../utils/loop', () => ({ loop: vi.fn() }));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const makeMockStore = (positions: Position[] = [], orders: any[] = [], markets: any[] = []) => ({
  positions,
  orders,
  markets,
  tickers: [],
  loaded: { balance: false, orders: false, markets: false, tickers: false, positions: false },
  balance: { total: 0, free: 0, used: 0, upnl: 0 },
  marketsBySymbol: Object.fromEntries(markets.map((m: any) => [m.symbol, m])),
  update: vi.fn(),
  subscribe: vi.fn(() => vi.fn()),
  setSetting: vi.fn(),
  updatePositions: vi.fn(),
  addOrUpdateOrders: vi.fn(),
  updateOrdersStatus: vi.fn(),
  addOrUpdatePositions: vi.fn(),
  addOrUpdatePositionsFull: vi.fn(),
  addOrUpdateOrder: vi.fn(),
  removeOrder: vi.fn(),
  removeOrders: vi.fn(),
  updateOrder: vi.fn(),
  updateOrderStatus: vi.fn(),
  addOrder: vi.fn(),
  setOrdersForSymbol: vi.fn(),
  updatePosition: vi.fn(),
  addOrUpdatePosition: vi.fn(),
  updateTicker: vi.fn(),
  updateMarket: vi.fn(),
  updateStarkConfig: vi.fn(),
  updateZkLinkConfig: vi.fn(),
  updateUserData: vi.fn(),
  updateSocketHealth: vi.fn(),
  serialize: vi.fn(),
  get displayedPositions() {
    return [];
  },
  get activeChasers() {
    return [];
  },
  get activeTWAPs() {
    return [];
  },
  get ordersRisk() {
    return {};
  },
  get latency() {
    return 0;
  },
  get options() {
    return { isHedged: false };
  },
  get updateIds() {
    return { positions: 0, orders: 0 };
  },
  get messageHandlerStats() {
    return {};
  },
  get starkConfig() {
    return undefined;
  },
  get zkLinkConfig() {
    return undefined;
  },
  get userData() {
    return undefined;
  },
  get tickersById() {
    return {};
  },
  get marketsById() {
    return {};
  },
  get tickersBySymbol() {
    return {};
  },
  get fetchedLeverage() {
    return {};
  },
  get socketHealth() {
    return undefined;
  },
});

const ARBUSDT_MARKET = {
  id: 'ARB/USDT',
  symbol: 'ARBUSDT',
  base: 'ARB',
  quote: 'USDT',
  active: true,
  precision: { amount: 0, price: 5 },
  limits: { amount: { min: 0, max: 10000000 }, leverage: { min: 1, max: 100 } },
};

const makePosition = (overrides: Partial<Position> = {}): Position => ({
  id: 'ARBUSDT:long',
  symbol: 'ARBUSDT',
  side: PositionSide.Long,
  entryPrice: 0.11,
  notional: 10,
  leverage: 20,
  unrealizedPnl: 0,
  contracts: 91,
  liquidationPrice: 0,
  positionMode: PositionMode.Hedged,
  marginType: MarginType.Cross,
  info: { positionId: '12345678', symbol: 'ARBUSDT', side: 'BUY', qty: '91' },
  ...overrides,
});

const createExchange = (store?: any) => {
  const mockStore = store || makeMockStore();
  const exchange = new BitunixExchange('bitunix' as any, { key: 'k', secret: 's' } as any, mockStore);
  return { exchange, store: mockStore };
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('BitunixExchange', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  // =========================================================================
  // fetchPositions - response format handling
  // =========================================================================
  describe('fetchPositions', () => {
    const RAW_POSITION = {
      positionId: '6674071536328511558',
      symbol: 'ARBUSDT',
      marginCoin: null,
      qty: '91',
      entryValue: '10.01',
      side: 'BUY',
      marginMode: 'CROSS',
      positionMode: 'HEDGE',
      leverage: 20,
      fee: '-0.006',
      funding: '0',
      realizedPNL: '-0.006',
      margin: '0.5',
      unrealizedPNL: '0.07',
      liqPrice: null,
      avgOpenPrice: '0.11',
    };

    it('should parse positions when API returns a direct array', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { data: [RAW_POSITION] } }),
      } as any;

      const positions = await exchange.fetchPositions();
      const real = positions.find((p) => p.symbol === 'ARBUSDT' && p.contracts > 0);

      expect(real).toBeDefined();
      expect(real!.contracts).toBe(91);
      expect(real!.side).toBe(PositionSide.Long);
      expect(real!.info).toBe(RAW_POSITION);
      expect(real!.info.positionId).toBe('6674071536328511558');
    });

    it('should parse positions when API returns positionList wrapper', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { data: { positionList: [RAW_POSITION] } } }),
      } as any;

      const positions = await exchange.fetchPositions();
      const real = positions.find((p) => p.symbol === 'ARBUSDT' && p.contracts > 0);

      expect(real).toBeDefined();
      expect(real!.contracts).toBe(91);
      expect(real!.info.positionId).toBe('6674071536328511558');
    });

    it('should return empty formatted positions when data is null', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { data: null } }),
      } as any;

      const positions = await exchange.fetchPositions();
      // Should only have fake positions (contracts: 0)
      expect(positions.every((p) => p.contracts === 0)).toBe(true);
    });

    it('should handle side: LONG as a long position', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));
      const pos = { ...RAW_POSITION, side: 'LONG' };

      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { data: [pos] } }),
      } as any;

      const positions = await exchange.fetchPositions();
      const real = positions.find((p) => p.contracts > 0);

      expect(real!.side).toBe(PositionSide.Long);
    });

    it('should handle side: SHORT as a short position', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));
      const pos = { ...RAW_POSITION, side: 'SHORT' };

      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { data: [pos] } }),
      } as any;

      const positions = await exchange.fetchPositions();
      const real = positions.find((p) => p.contracts > 0);

      expect(real!.side).toBe(PositionSide.Short);
    });

    it('should NOT populate positionIdCache from REST (only WS events should)', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.xhr = {
        get: vi.fn().mockResolvedValue({ data: { data: [RAW_POSITION] } }),
      } as any;

      await exchange.fetchPositions();

      // positionIdCache should remain empty — only WS position events populate it
      // REST data can be stale and would overwrite correct WS-sourced positionIds
      expect(exchange.positionIdCache.get('ARBUSDT:long')).toBeUndefined();
    });

    it('should return existing store positions on network error', async () => {
      const existingPositions = [makePosition()];
      const { exchange } = createExchange(makeMockStore(existingPositions, [], [ARBUSDT_MARKET]));

      exchange.xhr = {
        get: vi.fn().mockRejectedValue(new Error('Network Error')),
      } as any;

      const positions = await exchange.fetchPositions();

      expect(positions).toBe(existingPositions);
    });
  });

  // =========================================================================
  // positionIdCache & getPositionId
  // =========================================================================
  describe('positionIdCache', () => {
    it('getPositionId should prefer info.positionId over cache', () => {
      const position = makePosition({ info: { positionId: 'from-info' } });
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));
      exchange.positionIdCache.set('ARBUSDT:long', 'from-cache');

      const result = (exchange as any).getPositionId('ARBUSDT', PositionSide.Long);

      expect(result).toBe('from-info');
    });

    it('getPositionId should fall back to cache when info is undefined', () => {
      const position = makePosition({ info: undefined });
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));
      exchange.positionIdCache.set('ARBUSDT:long', 'from-cache');

      const result = (exchange as any).getPositionId('ARBUSDT', PositionSide.Long);

      expect(result).toBe('from-cache');
    });

    it('getPositionId should return undefined when no info and no cache', () => {
      const position = makePosition({ info: undefined });
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));

      const result = (exchange as any).getPositionId('ARBUSDT', PositionSide.Long);

      expect(result).toBeUndefined();
    });
  });

  // =========================================================================
  // formatCreateOrder - reduce-only positionId handling
  // =========================================================================
  describe('formatCreateOrder', () => {
    it('should throw "nothing to reduce" when no position exists', () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      expect(() =>
        (exchange as any).formatCreateOrder({
          symbol: 'ARBUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          amount: 91,
          reduceOnly: true,
        }),
      ).toThrow('nothing to reduce');
    });

    it('should throw "Missing positionId" when position exists but ID unavailable', () => {
      const position = makePosition({ info: undefined });
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));

      expect(() =>
        (exchange as any).formatCreateOrder({
          symbol: 'ARBUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          amount: 91,
          reduceOnly: true,
        }),
      ).toThrow('Missing positionId');
    });

    it('should use positionId from cache when info is missing', () => {
      const position = makePosition({ info: undefined });
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));
      exchange.positionIdCache.set('ARBUSDT:long', 'cached-id');

      const payloads = (exchange as any).formatCreateOrder({
        symbol: 'ARBUSDT',
        side: OrderSide.Sell,
        type: OrderType.Market,
        amount: 91,
        reduceOnly: true,
      });

      expect(payloads[0].positionId).toBe('cached-id');
      expect(payloads[0].tradeSide).toBe('CLOSE');
    });

    it('should use positionId from info when available', () => {
      const position = makePosition();
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));

      const payloads = (exchange as any).formatCreateOrder({
        symbol: 'ARBUSDT',
        side: OrderSide.Sell,
        type: OrderType.Market,
        amount: 91,
        reduceOnly: true,
      });

      expect(payloads[0].positionId).toBe('12345678');
    });
  });

  // =========================================================================
  // placeOrder - retry logic
  // =========================================================================
  describe('placeOrder retry', () => {
    it('should retry on "Missing positionId" and succeed after refresh', async () => {
      const position = makePosition({ info: undefined });
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));

      // First call: cache empty, will throw "Missing positionId"
      // fetchPositions in retry will populate cache
      const RAW = {
        positionId: 'refreshed-id',
        symbol: 'ARBUSDT',
        qty: '91',
        side: 'BUY',
        marginMode: 'CROSS',
        positionMode: 'HEDGE',
        leverage: 20,
        fee: '0',
        funding: '0',
        realizedPNL: '0',
        margin: '0',
        unrealizedPNL: '0',
        liqPrice: null,
        avgOpenPrice: '0.11',
        entryValue: '10',
      };

      exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { data: [RAW] } }) } as any;
      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({ data: { data: { orderId: 'order-123' } } }),
      } as any;

      const result = await exchange.placeOrder({
        symbol: 'ARBUSDT',
        side: OrderSide.Sell,
        type: OrderType.Market,
        amount: 91,
        reduceOnly: true,
      });

      expect(result).toContain('order-123');
    });

    it('should retry on "Position does not exist" API error', async () => {
      const position = makePosition();
      const { exchange } = createExchange(makeMockStore([position], [], [ARBUSDT_MARKET]));

      exchange.positionIdCache.set('ARBUSDT:long', 'stale-id');

      const RAW = {
        positionId: 'new-id',
        symbol: 'ARBUSDT',
        qty: '91',
        side: 'BUY',
        marginMode: 'CROSS',
        positionMode: 'HEDGE',
        leverage: 20,
        fee: '0',
        funding: '0',
        realizedPNL: '0',
        margin: '0',
        unrealizedPNL: '0',
        liqPrice: null,
        avgOpenPrice: '0.11',
        entryValue: '10',
      };

      exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { data: [RAW] } }) } as any;

      let callCount = 0;
      exchange.unlimitedXHR = {
        post: vi.fn().mockImplementation(async () => {
          callCount++;
          if (callCount === 1) {
            const err: any = new Error('Position does not exist');
            err.response = { data: { msg: 'Position does not exist' } };
            throw err;
          }
          return { data: { data: { orderId: 'order-456' } } };
        }),
      } as any;

      // Override emitAndLogError to prevent noise
      exchange.emitAndLogError = vi.fn() as any;

      const result = await exchange.placeOrder({
        symbol: 'ARBUSDT',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        price: 0.09,
        amount: 91,
        reduceOnly: true,
      });

      expect(result).toContain('order-456');
    });

    it('should NOT retry on "nothing to reduce" errors', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      await expect(
        exchange.placeOrder({
          symbol: 'ARBUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          amount: 91,
          reduceOnly: true,
        }),
      ).rejects.toThrow('nothing to reduce');
    });
  });

  // =========================================================================
  // placeOrderBatch - orderId extraction (data.data.orderId)
  // =========================================================================
  describe('placeOrderBatch orderId extraction', () => {
    it('should extract orderId from data.data.orderId for single orders', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({
          data: { code: 0, data: { orderId: 'real-order-id' }, msg: 'Success' },
        }),
      } as any;

      const result = await (exchange as any).placeOrderBatch([
        { symbol: 'ARBUSDT', qty: '91', side: 'BUY', orderType: 'LIMIT', price: '0.11', clientId: 'ts_fake' },
      ]);

      expect(result).toEqual(['real-order-id']);
    });

    it('should fall back to clientId when data.data.orderId is missing', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({
          data: { code: 0, data: {}, msg: 'Success' },
        }),
      } as any;

      const result = await (exchange as any).placeOrderBatch([
        { symbol: 'ARBUSDT', qty: '91', side: 'BUY', orderType: 'LIMIT', price: '0.11', clientId: 'ts_fallback' },
      ]);

      expect(result).toEqual(['ts_fallback']);
    });

    it('should extract orderId from data.data.orderId for TP/SL orders', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({
          data: { code: 0, data: { orderId: 'tpsl-order-id' }, msg: 'Success' },
        }),
      } as any;

      const result = await (exchange as any).placeOrderBatch([
        { symbol: 'ARBUSDT', positionId: '123', tpPrice: '0.15', tpStopType: 'LAST_PRICE', tpQty: '91' },
      ]);

      expect(result).toEqual(['tpsl-order-id']);
    });

    it('should extract orderId from data.data.orderId for position TP/SL orders', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({
          data: { code: 0, data: { orderId: 'pos-tpsl-id' }, msg: 'Success' },
        }),
      } as any;

      const result = await (exchange as any).placeOrderBatch([
        {
          symbol: 'ARBUSDT',
          positionId: '123',
          tpPrice: '0.15',
          isPositionOrder: true,
        },
      ]);

      expect(result).toEqual(['pos-tpsl-id']);
    });

    it('should handle batch order results from data.data array', async () => {
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({
          data: {
            code: 0,
            data: [{ orderId: 'batch-1' }, { orderId: 'batch-2' }],
            msg: 'Success',
          },
        }),
      } as any;

      const orders = [
        { symbol: 'ARBUSDT', qty: '10', side: 'BUY', orderType: 'LIMIT', price: '0.10' },
        { symbol: 'ARBUSDT', qty: '10', side: 'BUY', orderType: 'LIMIT', price: '0.11' },
      ];

      const result = await (exchange as any).placeOrderBatch(orders);

      expect(result).toEqual(['batch-1', 'batch-2']);
    });

    it('should extract orderIds from data.data.successList (real Bitunix shape)', async () => {
      // Real Bitunix /batch_order response:
      //   data.data = { successList: [{orderId, clientId}], failureList: [{errorMsg, errorCode}] }
      // The flat-array form above is a historical/test-only shape; production hits this one.
      const { exchange } = createExchange(makeMockStore([], [], [ARBUSDT_MARKET]));

      exchange.unlimitedXHR = {
        post: vi.fn().mockResolvedValue({
          data: {
            code: 0,
            data: {
              successList: [
                { orderId: 'real-1', clientId: 'ts_a' },
                { orderId: 'real-2', clientId: 'ts_b' },
              ],
              failureList: [],
            },
            msg: 'Success',
          },
        }),
      } as any;

      const orders = [
        { symbol: 'ARBUSDT', qty: '10', side: 'BUY', orderType: 'LIMIT', price: '0.10' },
        { symbol: 'ARBUSDT', qty: '10', side: 'BUY', orderType: 'LIMIT', price: '0.11' },
      ];

      const result = await (exchange as any).placeOrderBatch(orders);

      expect(result).toEqual(['real-1', 'real-2']);
    });
  });
});
