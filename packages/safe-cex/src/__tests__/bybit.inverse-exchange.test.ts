import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../exchanges/bybit/bybit.exchange';
import { ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { BybitDemoExchange } from '../exchanges/bybit/bybitdemo.exchange';
import { DefaultStore } from '../store/store.base';
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

  it('reads inverse ticker volume from the fields Bybit puts the coin and USD amounts in', () => {
    const { exchange, store } = create(true);
    store.update({ markets: [{ symbol: 'BTCUSD', id: 'BTCUSD', isInverse: true } as any] });
    const [ticker] = exchange.parseTickers([
      { symbol: 'BTCUSD', lastPrice: '80000', volume24h: '400000000', turnover24h: '5000' },
    ]);
    expect(ticker).toMatchObject({ volume: 5000, quoteVolume: 400000000 });
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

  it('subscribes candles for inverse symbols on the inverse socket', () => {
    const { exchange } = create(true);
    const linear = vi.spyOn(exchange.publicWebsocket, 'listenOHLCVs').mockImplementation(() => {});
    const inverse = vi.spyOn(exchange.publicWebsocketSecondary as any, 'listenOHLCVs').mockImplementation(() => {});

    exchange.listenOHLCVs({ symbols: ['BTCUSDT', 'BTCUSD', 'ETHUSD'], interval: '1m' as any }, () => {});
    expect(linear).toHaveBeenCalledWith(expect.objectContaining({ symbols: ['BTCUSDT'] }), expect.any(Function));
    expect(inverse).toHaveBeenCalledWith(
      expect.objectContaining({ symbols: ['BTCUSD', 'ETHUSD'] }),
      expect.any(Function),
    );
  });

  it('keeps inverse off Bybit demo even when the account asks for it', async () => {
    const { exchange } = create(true, BybitDemoExchange);
    const get = routeGets(exchange, { [`${ENDPOINTS.MARKETS}:linear`]: [linearRow] });
    await exchange.fetchMarkets();
    expect(categoriesAsked(get, ENDPOINTS.MARKETS)).not.toContain('inverse');
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
