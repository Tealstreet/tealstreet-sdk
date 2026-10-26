import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { BybitDemoExchange } from './reference/bybit/bybitdemo.exchange';
import { DefaultStore } from '../store/store.base';
import { tealstreetTransport } from '../transport/tealstreet/tealstreetTransport';
import { OrderSide, OrderType } from '../types';

const marketRow = (symbol: string, quoteCoin: string, settleCoin: string, qtyStep: string) => ({
  symbol,
  baseCoin: 'BTC',
  quoteCoin,
  settleCoin,
  status: 'Trading',
  lotSizeFilter: { qtyStep, minOrderQty: qtyStep, maxOrderQty: '1000000' },
  priceFilter: { tickSize: '0.5' },
  leverageFilter: { minLeverage: '1', maxLeverage: '100' },
  deliveryTime: '0',
});

const linearRow = marketRow('BTCUSDT', 'USDT', 'USDT', '0.001');
const inverseRow = marketRow('BTCUSD', 'USD', 'BTC', '1');

const ok = (list: unknown[]) => ({ data: { retCode: 0, retMsg: 'OK', result: { list } } });

const live: BybitExchange[] = [];
afterEach(() => live.splice(0).forEach((e) => e.dispose()));

const create = (enableInverseMarkets: boolean, Exchange: typeof BybitExchange = BybitExchange) => {
  const store = new DefaultStore();
  const exchange = new Exchange(
    'bybit',
    { key: 'k', secret: 's', enableInverseMarkets, tuning: { consoleLogging: false } } as any,
    store,
  );
  live.push(exchange);
  vi.spyOn(exchange as any, 'fetchFees').mockResolvedValue([]);
  return { exchange, store };
};

const routeGets = (exchange: BybitExchange, rows: Record<string, unknown[]>) => {
  const get = vi.fn((url: string, config?: any) => {
    if (config?.params?.status === 'PreLaunch') return Promise.resolve(ok([]));
    return Promise.resolve(ok(rows[`${url}:${config?.params?.category}`] ?? []));
  });
  (exchange.xhr.get as any) = get;
  return get;
};

const categoriesAsked = (get: ReturnType<typeof vi.fn>, url: string) =>
  get.mock.calls.filter(([u]) => u === url).map(([, config]) => config.params.category);

const inversePosition = {
  symbol: 'BTCUSD',
  side: 'Buy',
  positionIdx: 0,
  size: '1000',
  avgPrice: '80000',
  markPrice: '100000',
  positionValue: '0.0125',
  unrealisedPnl: '0.0025',
  leverage: '10',
  liqPrice: '',
};

describe('Bybit inverse contracts on the bybit connection', () => {
  it('routes inverse symbols to the inverse category whether or not the account opted in', () => {
    const { exchange } = create(false);
    expect(exchange.getCategoryForSymbol('BTCUSD')).toBe('inverse');
    expect(exchange.getCategoryForSymbol('ETHUSDZ26')).toBe('inverse');
    expect(exchange.getCategoryForSymbol('BTCUSDT')).toBe('linear');
    expect(exchange.getCategoryForSymbol('BTCPERP')).toBe('linear');
    expect(exchange.getCategoryForSymbol('BTC-26DEC26')).toBe('linear');
  });

  it('requests nothing inverse when the account has not opted in', async () => {
    const { exchange } = create(false);
    const get = routeGets(exchange, { [`${ENDPOINTS.MARKETS}:linear`]: [linearRow] });

    expect((await exchange.fetchMarkets()).map((m) => m.symbol)).toEqual(['BTCUSDT']);
    await exchange.fetchOrders();
    await exchange.fetchPositions();
    expect(get.mock.calls.some(([, config]) => config?.params?.category === 'inverse')).toBe(false);
  });

  it('adds inverse markets, marked isInverse, when opted in', async () => {
    const { exchange } = create(true);
    routeGets(exchange, {
      [`${ENDPOINTS.MARKETS}:linear`]: [linearRow],
      [`${ENDPOINTS.MARKETS}:inverse`]: [inverseRow],
    });

    const markets = await exchange.fetchMarkets();
    expect(markets.map((m) => [m.symbol, Boolean(m.isInverse)])).toEqual([
      ['BTCUSDT', false],
      ['BTCUSD', true],
    ]);
  });

  it('reads open orders and positions from both categories when opted in', async () => {
    const { exchange } = create(true);
    const get = routeGets(exchange, {});

    await exchange.fetchOrders();
    await exchange.fetchPositions();
    expect(categoriesAsked(get, ENDPOINTS.UNFILLED_ORDERS)).toContain('inverse');
    expect(categoriesAsked(get, ENDPOINTS.POSITIONS)).toContain('inverse');
  });

  it('keeps inverse positions the store holds when one poll of them fails', async () => {
    const { exchange, store } = create(true);
    const held = { id: 'p', symbol: 'BTCUSD', contracts: 0.0125 } as any;
    store.update({ positions: [held] });
    (exchange.xhr.get as any) = vi.fn((_url: string, config: any) =>
      config.params.category === 'inverse'
        ? Promise.resolve({ data: { retCode: 10002, retMsg: 'timeout' } })
        : Promise.resolve(ok([])),
    );

    expect(await exchange.fetchPositions()).toEqual([held]);
  });

  it('maps an inverse position to coin contracts at the mark, with USD notional and coin PnL', () => {
    const { exchange } = create(true);
    const position = exchange.mapPosition(inversePosition);
    expect(position).toMatchObject({ symbol: 'BTCUSD', contracts: 0.01, notional: 1000, unrealizedPnl: 0.0025 });
    expect(position.extra).toMatchObject({ contractsNative: 1000, unrealizedPnlNative: 0.0025 });
  });

  it('values inverse PnL in USD in the account balance', () => {
    const { exchange, store } = create(true);
    store.update({ positions: [exchange.mapPosition(inversePosition)] });
    const balance = exchange.formatBalance([{ totalEquity: '1250', coin: [] }]);
    expect(balance.upnl).toBeCloseTo(250);
    expect(balance.total).toBeCloseTo(1000);
  });

  it('keeps the balance PnL a number while a closed inverse position is still in the store', () => {
    const { exchange, store } = create(true);
    const closed = { ...inversePosition, size: '0', positionValue: '0', unrealisedPnl: '0' };
    store.update({ positions: [exchange.mapPosition(closed)] });
    expect(exchange.formatBalance([{ totalEquity: '1000', coin: [] }])).toMatchObject({ upnl: 0, total: 1000 });
  });

  it('sizes an inverse position at the ticker mark ahead of the last price, and values its PnL at that price', () => {
    const { exchange, store } = create(true);
    store.update({ tickers: [{ id: 'BTCUSD', symbol: 'BTCUSD', mark: 100000, last: 90000 } as any] });
    const position = exchange.mapPosition({ ...inversePosition, markPrice: '80000' });
    expect(position.contracts).toBe(0.01);

    store.update({ positions: [position] });
    expect(exchange.formatBalance([{ totalEquity: '1250', coin: [] }]).upnl).toBeCloseTo(250);
  });

  it('values inverse PnL in USD when position updates refresh the balance', () => {
    const { exchange, store } = create(true);
    store.update({
      balance: { total: 1000, upnl: 0, used: 0, free: 1000 } as any,
      positions: [exchange.mapPosition(inversePosition)],
    });
    exchange.syncBalanceUpnlFromPositions();
    expect(store.balance.upnl).toBeCloseTo(250);
  });

  it('reads inverse ticker volume from the fields Bybit puts the coin and USD amounts in, over REST and the socket', () => {
    const { exchange, store } = create(true);
    store.update({ markets: [{ symbol: 'BTCUSD', id: 'BTCUSD', isInverse: true } as any] });
    // Recorded 2026-10-15: the coin volume times the last price is not the USD volume.
    const frame = { symbol: 'BTCUSD', lastPrice: '80880', volume24h: '467074752', turnover24h: '5722' };
    const [ticker] = exchange.parseTickers([frame]);
    expect(ticker).toMatchObject({ volume: 5722, quoteVolume: 467074752 });

    store.update({ tickers: [{ ...ticker, volume: 0, quoteVolume: 0 }] });
    (exchange.publicWebsocketSecondary as any).handleTickersEvent({ data: frame });
    expect(store.tickersBySymbol.BTCUSD).toMatchObject({ volume: 5722, quoteVolume: 467074752 });
  });

  it('lists inverse tickers when opted in', async () => {
    const { exchange, store } = create(true);
    routeGets(exchange, {
      [`${ENDPOINTS.MARKETS}:linear`]: [linearRow],
      [`${ENDPOINTS.MARKETS}:inverse`]: [inverseRow],
      [`${ENDPOINTS.TICKERS}:linear`]: [{ symbol: 'BTCUSDT', lastPrice: '80000' }],
      [`${ENDPOINTS.TICKERS}:inverse`]: [{ symbol: 'BTCUSD', lastPrice: '80000' }],
    });
    store.update({ markets: await exchange.fetchMarkets() });
    expect((await exchange.fetchTickers()).map((t) => t.symbol)).toEqual(['BTCUSDT', 'BTCUSD']);
  });

  it('converts inverse closed PnL to coin contracts and USD notional', async () => {
    const { exchange } = create(true);
    const now = Date.now();
    routeGets(exchange, {
      [`${ENDPOINTS.USER_CLOSED_PNL}:inverse`]: [
        {
          symbol: 'BTCUSD',
          side: 'Sell',
          avgEntryPrice: '80000',
          avgExitPrice: '90000',
          closedSize: '1000',
          closedPnl: '0.0013',
          cumEntryValue: '0.0125',
          cumExitValue: '0.0111',
          createdTime: String(now - 1000),
          updatedTime: String(now - 1000),
        },
      ],
    });
    const [entry] = await exchange.fetchClosedPnl({ symbol: 'BTCUSD', from: now - 3_600_000, to: now });
    expect(Number(entry.notional)).toBe(1000);
    expect(Number(entry.contracts)).toBeCloseTo(0.0111, 4);
  });

  it('sends an inverse order in whole USD contracts on the inverse category', async () => {
    const { exchange, store } = create(true);
    routeGets(exchange, { [`${ENDPOINTS.MARKETS}:inverse`]: [inverseRow] });
    store.update({ markets: await exchange.fetchMarkets() });
    vi.spyOn(exchange as any, 'fetchPositionMode').mockResolvedValue('oneway');
    const post = vi.fn(() => Promise.resolve({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'o-1' } } }));
    (exchange.xhr.post as any) = post;
    (exchange.unlimitedXHR.post as any) = post;

    await exchange.placeOrder({ symbol: 'BTCUSD', side: OrderSide.Buy, type: OrderType.Limit, amount: 10.4, price: 1 });
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.CREATE_ORDER,
      expect.objectContaining({ category: 'inverse', symbol: 'BTCUSD', qty: '10' }),
    );
  });

  it('holds remaining in the coin on an inverse order and on its stop-loss and take-profit at their own prices', () => {
    const { exchange } = create(true);
    const orders = exchange.mapOrder({
      orderId: 'o-1',
      symbol: 'BTCUSD',
      side: 'Buy',
      orderType: 'Limit',
      orderStatus: 'New',
      price: '80000',
      qty: '1000',
      leavesQty: '1000',
      cumExecQty: '0',
      stopLoss: '50000',
      takeProfit: '100000',
    });
    expect(orders.map((o) => [o.price, o.remaining])).toEqual([
      [80000, 0.0125],
      [50000, 0.02],
      [100000, 0.01],
    ]);
  });

  it('records a placed inverse order with remaining in the coin', async () => {
    const { exchange, store } = create(true);
    routeGets(exchange, { [`${ENDPOINTS.MARKETS}:inverse`]: [inverseRow] });
    store.update({ markets: await exchange.fetchMarkets() });
    (exchange as any).recordRecentlyPlacedOrderFromOpts(
      'o-1',
      { symbol: 'BTCUSD', side: OrderSide.Buy, type: OrderType.Limit, amount: 1000, price: 80000 },
      { qty: '1000', price: '80000' },
    );
    expect((exchange as any).recentlyPlacedOrders.get('o-1').order).toMatchObject({
      amount: 1000,
      remaining: 0.0125,
      extra: { remainingNative: 1000 },
    });
  });

  it('sends a stop-loss for a whole inverse position to the position trading stop', async () => {
    const { exchange, store } = create(true);
    routeGets(exchange, { [`${ENDPOINTS.MARKETS}:inverse`]: [inverseRow] });
    store.update({ markets: await exchange.fetchMarkets() });
    store.update({ positions: [exchange.mapPosition(inversePosition)] });
    vi.spyOn(exchange as any, 'fetchPositionMode').mockResolvedValue('oneway');
    const post = vi.fn(() => Promise.resolve({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'o-1' } } }));
    (exchange.xhr.post as any) = post;
    (exchange.unlimitedXHR.post as any) = post;

    await exchange.placeOrder({
      symbol: 'BTCUSD',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 1000,
      price: 70000,
    });
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.SET_TRADING_STOP,
      expect.objectContaining({ category: 'inverse', symbol: 'BTCUSD', stopLoss: '70000' }),
    );
  });

  it('refuses a position mode change on an inverse symbol', async () => {
    const { exchange } = create(true);
    await expect(exchange.changePositionMode(true, 'BTCUSD')).rejects.toThrow('linear markets');
  });

  it('reads account-wide order history and income from both categories when opted in', async () => {
    const { exchange } = create(true);
    const get = routeGets(exchange, {});

    await exchange.fetchHistoricalOrders();
    await exchange.fetchIncomeHistory();
    expect(categoriesAsked(get, ENDPOINTS.USER_ORDERS)).toEqual(['linear', 'inverse']);
    expect(categoriesAsked(get, ENDPOINTS.TRANSACTION_LOG)).toEqual(['linear', 'inverse']);
  });

  it('returns the newest income across both categories, up to the limit', async () => {
    const { exchange } = create(true);
    const row = (symbol: string, transactionTime: number) => ({
      symbol,
      type: 'SETTLEMENT',
      funding: '-0.1',
      currency: 'USDT',
      transactionTime: `${transactionTime}`,
      id: `${symbol}-${transactionTime}`,
    });
    routeGets(exchange, {
      [`${ENDPOINTS.TRANSACTION_LOG}:linear`]: [row('BTCUSDT', 3000), row('BTCUSDT', 1000)],
      [`${ENDPOINTS.TRANSACTION_LOG}:inverse`]: [row('BTCUSD', 2000)],
    });

    const records = await exchange.fetchIncomeHistory({ limit: 2 });
    expect(records.map((r) => [r.symbol, r.timestamp])).toEqual([
      ['BTCUSDT', 3000],
      ['BTCUSD', 2000],
    ]);
  });

  it('subscribes candles and order books for inverse symbols on the inverse socket', () => {
    const { exchange } = create(true);
    const linear = exchange.publicWebsocket as any;
    const inverse = exchange.publicWebsocketSecondary as any;
    for (const method of ['listenOHLCVs', 'listenOHLCV', 'listenOrderBook']) {
      vi.spyOn(linear, method).mockImplementation(() => () => {});
      vi.spyOn(inverse, method).mockImplementation(() => () => {});
    }

    exchange.listenOHLCVs({ symbols: ['BTCUSDT', 'BTCUSD', 'ETHUSD'], interval: '1m' as any }, () => {});
    exchange.listenOHLCV({ symbol: 'BTCUSD', interval: '1m' as any }, () => {});
    exchange.listenOrderBook('BTCUSD', () => {});
    exchange.listenOrderBook('BTCUSDT', () => {});

    expect(linear.listenOHLCVs).toHaveBeenCalledWith(
      expect.objectContaining({ symbols: ['BTCUSDT'] }),
      expect.any(Function),
    );
    expect(inverse.listenOHLCVs).toHaveBeenCalledWith(
      expect.objectContaining({ symbols: ['BTCUSD', 'ETHUSD'] }),
      expect.any(Function),
    );
    expect(inverse.listenOHLCV).toHaveBeenCalledWith(
      expect.objectContaining({ symbol: 'BTCUSD' }),
      expect.any(Function),
    );
    expect(linear.listenOHLCV).not.toHaveBeenCalled();
    expect(inverse.listenOrderBook).toHaveBeenCalledWith('BTCUSD', expect.any(Function));
    expect(linear.listenOrderBook).toHaveBeenCalledWith('BTCUSDT', expect.any(Function));
  });

  it('opens the inverse socket once markets load and closes it with the connection', async () => {
    const { exchange } = create(true);
    for (const method of ['fetchAndCacheApiKeyInfo', 'tick', 'fetchOrdersPositionsAndBalance']) {
      vi.spyOn(exchange as any, method).mockResolvedValue([]);
    }
    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'fetchMarkets').mockResolvedValue([]);
    vi.spyOn(exchange as any, 'fetchTickersWithoutParse').mockResolvedValue([]);
    vi.spyOn(exchange as any, 'parseTickers').mockReturnValue([]);
    routeGets(exchange, {});
    const sockets = ['publicWebsocket', 'publicWebsocketSecondary', 'privateWebsocket'].map((name) => {
      const socket = { connectAndSubscribe: vi.fn(), dispose: vi.fn() };
      (exchange as any)[name] = socket;
      return socket;
    });

    await exchange.start();
    expect(sockets[1].connectAndSubscribe).toHaveBeenCalled();
    exchange.dispose();
    expect(sockets[1].dispose).toHaveBeenCalled();
  });

  it('connects a Bybit Brazil account to the Brazil inverse stream', () => {
    const exchange = new BybitExchange(
      'bybit',
      {
        key: 'k',
        secret: 's',
        enableInverseMarkets: true,
        bybitSiteId: 'BRA_BTL',
        transport: tealstreetTransport,
        tuning: { consoleLogging: false },
      } as any,
      new DefaultStore(),
    );
    live.push(exchange);
    const urls: string[] = [];
    for (const socket of [exchange.publicWebsocket, exchange.publicWebsocketSecondary] as any[]) {
      vi.spyOn(socket, 'createWebsocket').mockImplementation((url: any) => {
        urls.push(url);
        return { addEventListener: () => {}, close: () => {}, removeEventListener: () => {} };
      });
      socket.connectAndSubscribe();
    }
    expect(urls[0]).toContain('/bybitbra/v5/public/linear');
    expect(urls[1]).toContain('/bybitbra/v5/public/inverse');
  });

  it('keeps inverse off Bybit demo even when the account asks for it', async () => {
    const { exchange } = create(true, BybitDemoExchange);
    const get = routeGets(exchange, { [`${ENDPOINTS.MARKETS}:linear`]: [linearRow] });
    await exchange.fetchMarkets();
    expect(categoriesAsked(get, ENDPOINTS.MARKETS)).not.toContain('inverse');
  });

  it('keeps inverse off HyroTrader, whose simulator streams linear only, even when the account asks for it', async () => {
    vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL', 'https://simulator.test');
    vi.resetModules();
    try {
      const { HyrotraderExchange } = await import('./reference/bybit/hyrotrader.exchange');
      const { DefaultStore: Store } = await import('../store/store.base');
      const exchange = new HyrotraderExchange(
        'hyrotrader',
        { key: 'k', secret: 's', enableInverseMarkets: true, tuning: { consoleLogging: false } } as any,
        new Store(),
      );
      expect(exchange.publicWebsocketSecondary).toBeUndefined();
      exchange.dispose();
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('keeps inverse off when public data comes from a shared source', () => {
    const { exchange } = create(false);
    const shared = new BybitExchange(
      'bybit',
      { key: 'k', secret: 's', enableInverseMarkets: true, externalPublicData: {} } as any,
      new DefaultStore(),
    );
    live.push(shared);
    expect(shared.publicWebsocketSecondary).toBeUndefined();
    expect(exchange.publicWebsocketSecondary).toBeUndefined();
    expect(create(true).exchange.publicWebsocketSecondary).toBeDefined();
  });
});
