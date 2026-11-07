import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { mapBybitOptionInstrument } from './reference/bybit/bybit.options';
import { ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { BybitDemoExchange } from './reference/bybit/bybitdemo.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../types';

const OPTION = 'BTC-6NOV26-84000-C-USDT';

const optionRow = {
  symbol: OPTION,
  status: 'Trading',
  baseCoin: 'BTC',
  quoteCoin: 'USDT',
  settleCoin: 'USDT',
  optionsType: 'Call',
  deliveryTime: '1793952000000',
  priceFilter: { minPrice: '5', maxPrice: '1110000', tickSize: '5' },
  lotSizeFilter: { maxOrderQty: '500', minOrderQty: '0.01', qtyStep: '0.01' },
};

const linearRow = {
  symbol: 'BTCUSDT',
  baseCoin: 'BTC',
  quoteCoin: 'USDT',
  settleCoin: 'USDT',
  status: 'Trading',
  lotSizeFilter: { qtyStep: '0.001', minOrderQty: '0.001', maxOrderQty: '100' },
  priceFilter: { tickSize: '0.1' },
  leverageFilter: { minLeverage: '1', maxLeverage: '100' },
  deliveryTime: '0',
};

const ok = (list: unknown[], extra: Record<string, unknown> = {}) => ({
  data: { retCode: 0, retMsg: 'OK', result: { list, ...extra } },
});

const live: BybitExchange[] = [];
afterEach(() => live.splice(0).forEach((e) => e.dispose()));

const create = (enableOptionMarkets: boolean, Exchange: typeof BybitExchange = BybitExchange) => {
  const store = new DefaultStore();
  const exchange = new Exchange(
    'bybit',
    { key: 'k', secret: 's', enableOptionMarkets, tuning: { consoleLogging: false } } as any,
    store,
  );
  live.push(exchange);
  vi.spyOn(exchange as any, 'fetchFees').mockResolvedValue([]);
  return { exchange, store };
};

const routeGets = (exchange: BybitExchange, optionMarkets: () => Promise<unknown>) => {
  const get = vi.fn((url: string, config?: any) => {
    if (url === ENDPOINTS.OPTION_BASE_COINS) return Promise.resolve(ok([{ baseCoin: 'BTC' }]));
    if (url === ENDPOINTS.MARKETS && config.params.category === 'option') return optionMarkets();
    if (url === ENDPOINTS.MARKETS) return Promise.resolve(ok(config.params.status === 'PreLaunch' ? [] : [linearRow]));
    throw new Error(`unexpected GET ${url}`);
  });
  (exchange.xhr.get as any) = get;
  return get;
};

describe('Bybit options on the bybit connection', () => {
  it('routes an option symbol to the option category even when its market is not loaded', () => {
    const { exchange } = create(false);
    expect(exchange.getCategoryForSymbol(OPTION)).toBe('option');
    expect(exchange.getCategoryForSymbol('SOL-6NOV26-112d5-P')).toBe('option');
    expect(exchange.getCategoryForSymbol('NVDA-30OCT26-247.5-P-USDT')).toBe('option');
    expect(exchange.getCategoryForSymbol('BTCUSDT-16OCT26')).toBe('linear');
    expect(exchange.getCategoryForSymbol('BTCUSDT')).toBe('linear');
    expect(exchange.getCategoryForSymbol('BTC-26DEC26')).toBe('linear');
  });

  it('requests no option markets when the account has not opted in', async () => {
    const { exchange } = create(false);
    const get = routeGets(exchange, () => Promise.resolve(ok([optionRow])));

    const markets = await exchange.fetchMarkets();
    expect(markets.map((m) => m.symbol)).toEqual(['BTCUSDT']);
    expect(get.mock.calls.some(([url]) => url === ENDPOINTS.OPTION_BASE_COINS)).toBe(false);
  });

  it('adds option markets when opted in, and keeps them off the linear ticker stream', async () => {
    const { exchange, store } = create(true);
    routeGets(exchange, () => Promise.resolve(ok([optionRow], { nextPageCursor: '' })));

    const markets = await exchange.fetchMarkets();
    expect(markets.map((m) => m.symbol)).toEqual(['BTCUSDT', OPTION]);

    store.update({ markets });
    expect(exchange.getTickerSymbolsForCategory('linear')).toEqual(['BTCUSDT']);
  });

  it('keeps the loaded option markets when a refresh of them fails', async () => {
    const { exchange, store } = create(true);
    store.update({ markets: [mapBybitOptionInstrument(optionRow)!] });
    routeGets(exchange, () => Promise.reject(new Error('timeout')));

    const markets = await exchange.fetchMarkets();
    expect(markets.map((m) => m.symbol)).toEqual(['BTCUSDT', OPTION]);
  });

  it('keeps option positions the store holds when one poll of them fails', async () => {
    const { exchange, store } = create(true);
    const held = { id: 'p', symbol: OPTION, contracts: 0.01 } as any;
    store.update({ positions: [held] });
    (exchange.xhr.get as any) = vi.fn((_url: string, config: any) =>
      config.params.category === 'option'
        ? Promise.resolve({ data: { retCode: 10002, retMsg: 'timeout' } })
        : Promise.resolve(ok([])),
    );

    expect(await exchange.fetchPositions()).toEqual([held]);
  });

  describe('option orders', () => {
    const placing = () => {
      const { exchange, store } = create(true);
      store.update({ markets: [mapBybitOptionInstrument(optionRow)!] });
      const post = vi.fn((_url: string, payload: any) =>
        Promise.resolve({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'o-1' } }, payload }),
      );
      (exchange.xhr.post as any) = post;
      return { exchange, post };
    };
    const order = { symbol: OPTION, side: OrderSide.Buy, type: OrderType.Limit, amount: 0.01 };

    it('sends category option with a minted orderLinkId', async () => {
      const { exchange, post } = placing();
      await expect(
        exchange.placeOrder({ ...order, price: 5, timeInForce: OrderTimeInForce.PostOnly }),
      ).resolves.toEqual(['o-1']);
      const [url, payload] = post.mock.calls[0];
      expect(url).toBe(ENDPOINTS.CREATE_ORDER);
      expect(payload).toMatchObject({
        category: 'option',
        symbol: OPTION,
        orderType: 'Limit',
        price: '5',
        qty: '0.01',
      });
      expect(payload.orderLinkId).toEqual(expect.any(String));
      expect(payload.positionIdx).toBeUndefined();
    });

    it('sends orderIv and no price when priced by implied volatility', async () => {
      const { exchange, post } = placing();
      await exchange.placeOrder({ ...order, impliedVolatility: 0.45 });
      expect(post.mock.calls[0][1]).toMatchObject({ orderIv: '0.45' });
      expect(post.mock.calls[0][1].price).toBeUndefined();
    });

    it.each([
      ['stopLoss', { stopLoss: 10 }],
      ['triggerPrice', { triggerPrice: 10 }],
    ])('refuses %s before sending anything', async (field, extra) => {
      const { exchange, post } = placing();
      await expect(exchange.placeOrder({ ...order, price: 5, ...extra })).rejects.toThrow(field);
      expect(post).not.toHaveBeenCalled();
    });

    it('refuses impliedVolatility on a market order before sending anything', async () => {
      const { exchange, post } = placing();
      await expect(
        exchange.placeOrder({ ...order, type: OrderType.Market, impliedVolatility: 0.45 }),
      ).rejects.toThrow('limit option orders only');
      expect(post).not.toHaveBeenCalled();
    });

    it('rejects a whole batch before any write when one option order cannot be placed', async () => {
      const { exchange, post } = placing();
      await expect(
        exchange.placeOrders([
          { symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 0.001, price: 50_000 },
          { ...order, price: 5, stopLoss: 10 },
        ]),
      ).rejects.toThrow('stopLoss');
      expect(post).not.toHaveBeenCalled();
    });
  });

  it('maps an option position with its greeks, leverage 1 and no liquidation price', () => {
    const { exchange } = create(true);
    const position = exchange.mapPosition({
      symbol: OPTION,
      side: 'Buy',
      size: '0.01',
      avgPrice: '2900',
      positionValue: '29',
      unrealisedPnl: '0.5',
      leverage: '',
      liqPrice: '',
      delta: '0.0051',
      gamma: '0.0000005',
      vega: '0.84',
      theta: '-0.64',
    });
    expect(position).toMatchObject({ symbol: OPTION, contracts: 0.01, leverage: 1, liquidationPrice: 0 });
    expect(position.option?.greeks).toMatchObject({ delta: 0.0051, vega: 0.84, theta: -0.64 });
  });

  it('sends option orders in a batch one at a time through the option path', async () => {
    const { exchange, store } = create(true);
    store.update({ markets: [mapBybitOptionInstrument(optionRow)!] });
    const post = vi.fn((_url: string, payload: any) =>
      Promise.resolve({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'o-1' } }, payload }),
    );
    (exchange.xhr.post as any) = post;

    await exchange.placeOrders([
      { symbol: OPTION, side: OrderSide.Buy, type: OrderType.Limit, amount: 0.01, price: 5 },
    ]);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_ORDER, expect.objectContaining({ category: 'option' }));
  });

  it('refuses closed PnL and leverage for option symbols', async () => {
    const { exchange } = create(true);
    await expect(exchange.fetchClosedPnl({ symbol: OPTION })).rejects.toThrow('no closed PnL for options');
    await expect(exchange.setLeverage(OPTION, 5)).rejects.toThrow('no leverage for options');
  });

  it('refuses option chains on an account that has not opted in', async () => {
    await expect(create(false).exchange.fetchOptionChain({ underlying: 'BTC' })).rejects.toThrow(
      'Options are not enabled',
    );
  });

  it('keeps options off Bybit demo even when the account asks for them', async () => {
    const { exchange } = create(true, BybitDemoExchange);
    const get = routeGets(exchange, () => Promise.resolve(ok([optionRow])));
    await exchange.fetchMarkets();
    expect(get.mock.calls.some(([url]) => url === ENDPOINTS.OPTION_BASE_COINS)).toBe(false);
  });
});
