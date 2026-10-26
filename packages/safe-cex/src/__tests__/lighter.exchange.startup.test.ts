import { afterEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from './reference/lighter/lighter.exchange';
import { ENDPOINTS } from '../exchanges/lighter/lighter.types';
import { DefaultStore } from '../store/store.base';
import { MarginType, PositionSide } from '../types';
import type { Balance, ExchangeOptions, Market } from '../types';

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
};

const createDeferred = <T>(): Deferred<T> => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((innerResolve) => {
    resolve = innerResolve;
  });
  return { promise, resolve };
};

const flushPromises = async (passes = 5) => {
  for (let i = 0; i < passes; i += 1) {
    await Promise.resolve();
  }
};

const createExchange = (): LighterExchange => {
  const options = {
    key: 'lighter-key',
    secret: 'lighter-secret',
    apiKeyIndex: '1',
    applicationId: '123',
    testnet: false,
  } as ExchangeOptions;

  return new LighterExchange('lighter', options, new DefaultStore());
};

const coreMarkets: Market[] = [
  {
    id: '1',
    symbol: 'BTC/USDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: {
      amount: { min: 0.001, max: 1000 },
      leverage: { min: 1, max: 50 },
    },
    isExpiring: false,
    info: {
      amountDecimals: 3,
      priceDecimals: 1,
    },
  },
];

describe('LighterExchange startup sequencing', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('keeps Lighter perp markets USDC-quoted while filtering spot order books', async () => {
    const exchange = createExchange();
    (exchange as any).lighterApiClient = {
      get: vi.fn().mockResolvedValue({
        code: 200,
        order_books: [
          {
            market_id: 1,
            symbol: 'BTC',
            market_type: 'perp',
            status: 'active',
            supported_size_decimals: 3,
            supported_price_decimals: 1,
            min_base_amount: '0.001',
            order_quote_limit: '1000000',
          },
          {
            market_id: 2,
            symbol: 'BTCUSDC',
            market_type: 'perp',
            status: 'active',
            supported_size_decimals: 3,
            supported_price_decimals: 1,
          },
          {
            market_id: 3,
            symbol: 'ETH/USDT',
            market_type: 'perp',
            status: 'active',
            supported_size_decimals: 2,
            supported_price_decimals: 2,
          },
          {
            market_id: 4,
            symbol: 'SOL/USDC',
            market_type: 'spot',
            status: 'active',
            supported_size_decimals: 2,
            supported_price_decimals: 2,
          },
        ],
      }),
    };

    const markets = await exchange.fetchMarkets();

    expect(markets.map((market) => market.symbol)).toEqual(['BTC', 'BTCUSDC', 'ETH/USDT']);
    expect(markets[0]).toMatchObject({ base: 'BTC', quote: 'USDC', isSpot: false });
    expect(markets[1]).toMatchObject({ base: 'BTC', quote: 'USDC', isSpot: false });
    expect(markets[2]).toMatchObject({ base: 'ETH', quote: 'USDT', isSpot: false });
  });

  it('uses orderBookDetails margin config for flat isolated-only Lighter markets', async () => {
    const exchange = createExchange();
    const updateLeverage = vi.fn().mockResolvedValue(undefined);
    (exchange as any).lighterApiClient = {
      get: vi.fn().mockResolvedValue({
        code: 200,
        order_book_details: [
          {
            market_id: 1,
            symbol: 'BTC',
            market_type: 'perp',
            status: 'active',
            supported_size_decimals: 5,
            supported_price_decimals: 1,
            min_base_amount: '0.00020',
            min_quote_amount: '10.000000',
            order_quote_limit: '281474976.710655',
            min_initial_margin_fraction: 200,
            market_config: { market_margin_mode: 0 },
          },
          {
            market_id: 182,
            symbol: 'H100',
            market_type: 'perp',
            status: 'active',
            supported_size_decimals: 2,
            supported_price_decimals: 4,
            min_base_amount: '2.00',
            min_quote_amount: '10.000000',
            order_quote_limit: '5000000.000000',
            min_initial_margin_fraction: 1000,
            market_config: { market_margin_mode: 1 },
          },
          {
            market_id: 183,
            symbol: 'CTR',
            market_type: 'perp',
            status: 'active',
            supported_size_decimals: 1,
            supported_price_decimals: 5,
            min_base_amount: '20.0',
            min_quote_amount: '10.000000',
            order_quote_limit: '5000000.000000',
            min_initial_margin_fraction: 3333,
            market_config: { market_margin_mode: 1 },
          },
        ],
      }),
    };

    const markets = await exchange.fetchMarkets();
    exchange.store.update({ markets });
    (exchange as any).ensureSignerAdapter = vi.fn().mockResolvedValue({ updateLeverage });
    vi.spyOn(exchange as any, 'syncLeverageFromExchange').mockResolvedValue(undefined);

    expect(exchange.store.marketsBySymbol.H100.limits.leverage.max).toBe(10);
    expect(exchange.store.marketsBySymbol.CTR.limits.leverage.max).toBeCloseTo(3.0003, 4);

    await exchange.setLeverage('H100', 50);
    await exchange.setLeverage('CTR', 50);

    expect(updateLeverage).toHaveBeenNthCalledWith(1, {
      marketId: 182,
      leverage: 10,
      marginMode: 1,
    });
    expect(updateLeverage).toHaveBeenNthCalledWith(2, {
      marketId: 183,
      leverage: 3,
      marginMode: 1,
    });
  });

  it('downgrades transient private snapshot transport failures to warnings', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    const log = vi.spyOn(exchange, 'log');
    vi.spyOn(exchange, 'fetchPositions').mockRejectedValue(
      Object.assign(new Error('No response received from server'), {
        code: 'NO_RESPONSE',
      }),
    );
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);

    await (exchange as any).fetchAndSetPrivateSnapshotHelper();

    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(log).toHaveBeenCalledWith(
      expect.stringContaining('Private snapshot fetch skipped after transient REST transport failure'),
      expect.anything(),
    );
  });

  it('keeps unexpected private snapshot failures on the exchange error path', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    const error = new Error('unexpected payload shape');
    vi.spyOn(exchange, 'fetchPositions').mockRejectedValue(error);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);

    await (exchange as any).fetchAndSetPrivateSnapshotHelper();

    expect(emitAndLogError).toHaveBeenCalledWith(error);
  });

  it('does not treat account position initial margin as the market max leverage cap', async () => {
    const exchange = createExchange();
    exchange.store.update({ markets: coreMarkets });
    (exchange as any).fetchAccountSnapshot = vi.fn().mockResolvedValue({
      positions: [
        {
          market_id: 1,
          position: '0',
          avg_entry_price: '0',
          initial_margin_fraction: 5000,
          margin_mode: 0,
        },
      ],
    });

    const positions = await exchange.fetchPositions();

    expect(exchange.store.marketsBySymbol['BTC/USDT'].limits.leverage.max).toBe(50);
    expect(positions.find((position) => position.symbol === 'BTC/USDT')?.leverage).toBe(2);
  });

  it('derives isolated Lighter liquidation price when API returns a profit-side value', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: '183',
          symbol: 'CTR',
          base: 'CTR',
          quote: 'USDC',
          active: true,
          precision: { amount: 0.1, price: 0.00001 },
          limits: {
            amount: { min: 20, max: 5000000 },
            leverage: { min: 1, max: 3 },
          },
          isExpiring: false,
          isSpot: false,
          info: {
            amountDecimals: 1,
            priceDecimals: 5,
            maintenance_margin_fraction: 2000,
          },
        },
      ],
    });

    const position = exchange.mapPosition({
      market_id: 183,
      position: '53.9',
      avg_entry_price: '0.01857',
      initial_margin_fraction: '0.3333',
      margin_mode: 1,
      liquidation_price: '0.01863',
      unrealized_pnl: '0.20',
    });

    expect(position).toMatchObject({
      symbol: 'CTR',
      side: PositionSide.Long,
      marginType: MarginType.Isolated,
    });
    expect(position!.liquidationPrice).toBeLessThan(position!.entryPrice);
    expect(position!.liquidationPrice).toBeCloseTo(0.015475, 6);
  });

  it('starts public websocket before signer warmup settles', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const signerDeferred = createDeferred<any>();
    const initialBalance: Balance = {
      total: 1000,
      free: 1000,
      used: 0,
      upnl: 0,
      info: {},
    };

    (exchange as any).initExternalPublicData = vi.fn().mockResolvedValue(false);
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue(coreMarkets);
    (exchange as any).ensureSignerAdapter = vi.fn().mockReturnValue(signerDeferred.promise);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue(initialBalance);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);

    const publicConnectSpy = vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    const privateConnectSpy = vi
      .spyOn(exchange.privateWebsocket, 'connectAndSubscribe')
      .mockResolvedValue(undefined);

    const startPromise = exchange.start();
    await flushPromises();

    expect(publicConnectSpy).toHaveBeenCalledTimes(1);
    expect(privateConnectSpy).not.toHaveBeenCalled();

    const signerAdapter = { createOrder: vi.fn() };
    (exchange as any).signerAdapter = signerAdapter;
    signerDeferred.resolve(signerAdapter);
    await startPromise;

    expect(privateConnectSpy).toHaveBeenCalledTimes(1);
  });

  it('coalesces account snapshots and can defer account limits during startup balance fetch', async () => {
    const exchange = createExchange();
    (exchange as any).accountIndex = 123n;
    exchange.store.update({
      markets: coreMarkets,
      tickers: [],
    });
    const getMock = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.ACCOUNT) {
        return Promise.resolve({
          code: 200,
          accounts: [
            {
              positions: [],
              total_asset_value: '1000',
              available_balance: '900',
            },
          ],
        });
      }
      if (endpoint === ENDPOINTS.ACCOUNT_LIMITS) {
        return Promise.resolve({ code: 200 });
      }
      return Promise.resolve({});
    });
    (exchange as any).lighterApiClient = { get: getMock };

    const [positions, balance] = await Promise.all([
      exchange.fetchPositions(),
      exchange.fetchRawBalance({ includeAccountLimits: false }),
    ]);

    expect(positions).toHaveLength(1);
    expect(balance.free).toBe(900);
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.ACCOUNT)).toHaveLength(1);
    expect(getMock.mock.calls.some(([endpoint]) => endpoint === ENDPOINTS.ACCOUNT_LIMITS)).toBe(false);
  });

  it('does not reuse a recent account snapshot after private stream invalidation', async () => {
    const exchange = createExchange();
    (exchange as any).accountIndex = 123n;
    const accountSnapshots = [
      {
        code: 200,
        accounts: [{ positions: [], total_asset_value: '1000', available_balance: '900' }],
      },
      {
        code: 200,
        accounts: [{ positions: [], total_asset_value: '1200', available_balance: '1100' }],
      },
    ];
    const getMock = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.ACCOUNT) {
        return Promise.resolve(accountSnapshots.shift());
      }
      return Promise.resolve({});
    });
    (exchange as any).lighterApiClient = { get: getMock };

    const firstBalance = await exchange.fetchRawBalance({
      allowRecentAccountSnapshot: true,
      includeAccountLimits: false,
    });
    const cachedBalance = await exchange.fetchRawBalance({
      allowRecentAccountSnapshot: true,
      includeAccountLimits: false,
    });

    expect(firstBalance.free).toBe(900);
    expect(cachedBalance.free).toBe(900);
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.ACCOUNT)).toHaveLength(1);

    exchange.invalidateAccountSnapshotCache();
    const refreshedBalance = await exchange.fetchRawBalance({
      allowRecentAccountSnapshot: true,
      includeAccountLimits: false,
    });

    expect(refreshedBalance.free).toBe(1100);
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.ACCOUNT)).toHaveLength(2);
  });

  it('coalesces concurrent fetchOrders calls into one accountActiveOrders request', async () => {
    const exchange = createExchange();
    (exchange as any).accountIndex = 123n;
    (exchange as any).getPrivateWsAuthToken = vi.fn().mockResolvedValue('token');
    const ordersDeferred = createDeferred<any>();
    const getMock = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.OPEN_ORDERS) {
        return ordersDeferred.promise;
      }
      return Promise.resolve({});
    });
    (exchange as any).lighterApiClient = { get: getMock };

    const firstFetch = exchange.fetchOrders();
    const secondFetch = exchange.fetchOrders();
    await flushPromises();

    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.OPEN_ORDERS)).toHaveLength(1);

    ordersDeferred.resolve({ orders: [] });
    await expect(Promise.all([firstFetch, secondFetch])).resolves.toEqual([[], []]);
  });

  it('delays the first tick after startup snapshot and skips startup accountLimits', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const initialBalance: Balance = {
      total: 1000,
      free: 1000,
      used: 0,
      upnl: 0,
      info: {},
    };

    (exchange as any).initExternalPublicData = vi.fn().mockResolvedValue(false);
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue(coreMarkets);
    (exchange as any).ensureSignerAdapter = vi.fn().mockResolvedValue({ createOrder: vi.fn() });
    (exchange as any).getTickInterval = vi.fn(() => 2_000);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    const fetchRawBalanceSpy = vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue(initialBalance);
    const tickSpy = vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockResolvedValue(undefined);

    await exchange.start();

    expect(fetchRawBalanceSpy).toHaveBeenCalledWith({
      allowRecentAccountSnapshot: true,
      includeAccountLimits: false,
    });
    expect(tickSpy).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1_999);
    expect(tickSpy).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    await vi.runOnlyPendingTimersAsync();
    expect(tickSpy).toHaveBeenCalledTimes(1);
  });

  it('caps the first delayed tick so startup is not stretched by browser slow tick intervals', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const initialBalance: Balance = {
      total: 1000,
      free: 1000,
      used: 0,
      upnl: 0,
      info: {},
    };

    (exchange as any).initExternalPublicData = vi.fn().mockResolvedValue(false);
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue(coreMarkets);
    (exchange as any).ensureSignerAdapter = vi.fn().mockResolvedValue({ createOrder: vi.fn() });
    (exchange as any).getTickInterval = vi.fn(() => 30_000);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue(initialBalance);
    const tickSpy = vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockResolvedValue(undefined);

    await exchange.start();

    await vi.advanceTimersByTimeAsync(1_999);
    expect(tickSpy).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    await vi.runOnlyPendingTimersAsync();
    expect(tickSpy).toHaveBeenCalledTimes(1);
  });
});
