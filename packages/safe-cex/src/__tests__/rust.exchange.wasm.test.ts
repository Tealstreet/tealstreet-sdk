import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { OrderBook } from '../types';

import { afterEach, describe, expect, it } from 'vitest';

import { BaseExchange } from '../exchanges/base';
import { HYROTRADER_PROD_URL, HYROTRADER_SIMULATOR_URL } from '../exchanges/bybit/bybit.types';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../types';

const EXCHANGE_INFO = JSON.stringify({
  symbols: [
    {
      symbol: 'BTCUSDT',
      status: 'TRADING',
      baseAsset: 'BTC',
      quoteAsset: 'USDT',
      marginAsset: 'USDT',
      contractType: 'PERPETUAL',
      filters: [
        { filterType: 'PRICE_FILTER', tickSize: '0.10' },
        { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
        { filterType: 'MARKET_LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '120' },
      ],
    },
  ],
});

const RESPONSES: Record<string, string> = {
  exchangeInfo: EXCHANGE_INFO,
  '24hr': '[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]',
  bookTicker: '[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]',
  premiumIndex:
    '[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]',
  depth: '{"lastUpdateId":100,"bids":[["99.0","5"]],"asks":[["101.0","5"]]}',
  leverageBracket: '[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]',
  listenKey: '{"listenKey":"key"}',
  dual: '{"dualSidePosition":false}',
  positionRisk: '[]',
  symbolConfig: '[{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":20}]',
  account:
    '{"assets":[{"asset":"USDT","walletBalance":"1000","availableBalance":"900","initialMargin":"100","unrealizedProfit":"0"}]}',
  openOrders: '[]',
  openAlgoOrders: '[]',
  order: '{"orderId":4242,"status":"NEW"}',
};

/** Serves canned Binance responses; sockets open at once and can be fed frames. */
const recordedTransport = (overrides: Record<string, { status: number; body: string }> = {}) => {
  const sockets = new Map<string, (text: string) => void>();
  const requests: string[] = [];
  const transport: Transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      sockets.set(new URL(url).pathname, onMessage);
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url }) => {
      requests.push(`${method} ${url}`);
      const name = new URL(url).pathname.split('/').pop()!;
      if (overrides[name]) {
        const { status, body } = overrides[name];
        return { status, headers: '', body: new TextEncoder().encode(body) };
      }
      const body = RESPONSES[name];
      return { status: body ? 200 : 404, headers: '', body: new TextEncoder().encode(body ?? '{}') };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, sockets, requests };
};

/** Bybit V5 envelopes for one linear market and an empty unified account. */
const bybitTransport = (category: 'linear' | 'inverse' | 'spot' = 'linear') => {
  const sockets = new Map<string, (text: string) => void>();
  const writes: Array<{ path: string; body?: string }> = [];
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  const socketUrls: string[] = [];
  const symbol = category === 'inverse' ? 'BTCUSD' : 'BTCUSDT';
  const list = (rows: unknown[]) => ({ list: rows, nextPageCursor: '' });
  const transport: Transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      socketUrls.push(url);
      sockets.set(new URL(url).pathname, onMessage);
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url, body }) => {
      requests.push({ method, url, body });
      const { pathname: path, searchParams } = new URL(url);
      if (method !== 'GET') writes.push({ path, body });
      let result: unknown;
      let retExtInfo = {};
      switch (path) {
        case '/v5/market/instruments-info':
          result = list(
            searchParams.has('status')
              ? []
              : [
                  {
                    symbol,
                    status: 'Trading',
                    baseCoin: 'BTC',
                    quoteCoin: category === 'inverse' ? 'USD' : 'USDT',
                    priceFilter: { tickSize: '0.10' },
                    lotSizeFilter:
                      category === 'spot'
                        ? { basePrecision: '0.000001', minOrderQty: '0.00001', maxLimitOrderQty: '120', maxMarketOrderQty: '12' }
                        : { qtyStep: category === 'inverse' ? '1' : '0.001', minOrderQty: '0.001', maxOrderQty: '120' },
                  },
                ],
          );
          break;
        case '/v5/market/tickers':
          result = list([{ symbol, lastPrice: '90000', bid1Price: '89999.9', ask1Price: '90000.1' }]);
          break;
        case '/v5/account/info':
          result = { marginMode: 'REGULAR_MARGIN', unifiedMarginStatus: 6 };
          break;
        case '/v5/account/wallet-balance':
          result = list([
            {
              accountType: 'UNIFIED',
              totalWalletBalance: '1000',
              totalAvailableBalance: '1000',
              totalInitialMargin: '0',
              totalPerpUPL: '0',
            },
          ]);
          break;
        case '/v5/order/create':
          result = { orderId: '4242' };
          break;
        case '/v5/order/create-batch':
          result = list([{ orderId: '4242' }]);
          retExtInfo = { list: [{ code: 0, msg: 'OK' }] };
          break;
        case '/v5/account/fee-rate':
        case '/v5/order/realtime':
          result = list([]);
          break;
        case '/v5/position/list': {
          const symbol = new URL(url).searchParams.get('symbol');
          result = list(symbol ? [{ symbol, positionIdx: 0, size: '0' }] : []);
          break;
        }
        default:
          throw new Error(`Unexpected Bybit request: ${method} ${path}`);
      }
      return {
        status: 200,
        body: new TextEncoder().encode(JSON.stringify({ retCode: 0, retMsg: 'OK', result, retExtInfo })),
      };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, sockets, writes, requests, socketUrls };
};

const until = async (check: () => boolean) => {
  for (let i = 0; i < 200 && !check(); i++) await new Promise((resolve) => setTimeout(resolve, 10));
  expect(check()).toBe(true);
};

describe('RustEngineExchange', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => exchange?.dispose());

  it('fills the store from engine frames and serves books to listeners', async () => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('binance', { key: '', secret: '', publicDataOnly: true }, store);

    await exchange.start();
    expect(store.markets.map((m) => m.symbol)).toEqual(['BTCUSDT']);
    expect(store.markets[0].limits.amount.max).toBe(120);
    expect(store.tickersBySymbol.BTCUSDT.bid).toBe(99.9);

    const books: OrderBook[] = [];
    exchange.listenOrderBook('BTCUSDT', (book) => books.push(book));
    await until(() => tape.sockets.has('/public/ws'));
    const publicSocket = tape.sockets.get('/public/ws')!;
    publicSocket('{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":90,"u":105,"pu":89,"b":[["98.5","1"]],"a":[]}');
    await until(() => tape.requests.some((r) => r.includes('/fapi/v1/depth')));
    publicSocket('{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":106,"u":107,"pu":105,"b":[["98.7","2"]],"a":[]}');
    await until(() => books.length > 0);
    const book = books[books.length - 1];
    expect(book.bids[0]).toEqual({ price: 99, amount: 5, total: 5 });
    expect(book.asks[0]).toEqual({ price: 101, amount: 5, total: 5 });

    publicSocket('{"e":"bookTicker","s":"BTCUSDT","b":"99.5","a":"99.6"}');
    await until(() => store.tickersBySymbol.BTCUSDT.bid === 99.5);
  });

  it('turns trading methods into engine commands and loads the account', async () => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, store);

    await exchange.start();
    expect(store.balance).toMatchObject({ total: 1000, free: 900, used: 100 });
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 20, marginType: 'cross' });
    expect(store.markets[0].limits.leverage.max).toBe(125);

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.0123,
      price: 90000.04,
    });
    expect(ids).toEqual(['4242']);
    const placed = tape.requests.find((r) => r.startsWith('POST') && r.includes('/fapi/v1/order?'))!;
    expect(placed).toContain('quantity=0.012');
    expect(placed).toContain('price=90000');
    expect(placed).toContain('signature=');
  });

  it('honors Bybit client IDs through the inherited helpers and finds streamed orders', async () => {
    const tape = bybitTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, store);
    await exchange.start();
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.012, price: 90000 };
    expect(exchange.mintClientOrderId(opts)).toMatch(/^[A-Za-z0-9_-]{1,36}$/);

    const clientOrderId = 'Caller_Bybit-01';
    expect(await exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId })).toEqual({
      orderIds: ['4242'],
      clientOrderId,
    });
    expect(tape.writes).toHaveLength(1);
    expect(tape.writes[0].path).toBe('/v5/order/create-batch');
    expect(JSON.parse(tape.writes[0].body!).request).toEqual([
      expect.objectContaining({ orderLinkId: clientOrderId, symbol: 'BTCUSDT', qty: '0.012' }),
    ]);

    await until(() => tape.sockets.has('/v5/private'));
    tape.sockets.get('/v5/private')!(
      JSON.stringify({
        topic: 'order',
        data: [
          {
            orderId: '4242',
            orderLinkId: clientOrderId,
            symbol: 'BTCUSDT',
            orderStatus: 'New',
            orderType: 'Limit',
            side: 'Buy',
            price: '90000',
            qty: '0.012',
            leavesQty: '0.012',
            cumExecQty: '0',
          },
        ],
      }),
    );
    await until(() => store.orders.some((order) => order.clientOrderId === clientOrderId));
    expect(await exchange.findOrderByClientOrderId('BTCUSDT', clientOrderId)).toMatchObject({
      found: true,
      order: { id: '4242', clientOrderId, status: OrderStatus.Open },
    });
    expect(await exchange.findOrderByClientOrderId('BTCUSDT', 'missing')).toEqual({ found: false, definitive: false });
    expect(tape.writes).toHaveLength(1);
  });

  it('rejects unhonorable Bybit client IDs and whole batches before any write', async () => {
    const tape = bybitTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, new DefaultStore());
    await exchange.start();
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.012, price: 90000 };
    for (const clientOrderId of ['', 'invalid.id', 'a'.repeat(37)]) {
      await expect(exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId })).rejects.toThrow(
        'clientOrderId must match',
      );
    }
    await expect(
      exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: 'split', amount: 120.001 }),
    ).rejects.toThrow('max size');
    await expect(
      exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: 'bracket', stopLoss: 80000 }),
    ).rejects.toThrow('order shape');
    expect(exchange.mintClientOrderId({ ...opts, amount: 120.001 })).toBeUndefined();
    expect(exchange.mintClientOrderId({ ...opts, stopLoss: 80000 })).toBeUndefined();
    await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'invalid.id' }])).rejects.toThrow(
      'clientOrderId must match',
    );
    await expect(
      exchange.placeOrders([
        { ...opts, clientOrderId: 'same' },
        { ...opts, clientOrderId: 'same' },
      ]),
    ).rejects.toThrow('duplicated in one batch');
    expect(tape.writes).toEqual([]);
  });

  it('routes inverse native contract quantities and caller IDs through the shared crate', async () => {
    const tape = bybitTransport('inverse');
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('bybitinverse', { key: 'k', secret: 's' }, store);
    await exchange.start();
    expect(store.marketsBySymbol.BTCUSD).toMatchObject({ isInverse: true, precision: { amount: 1 } });
    const opts = { symbol: 'BTCUSD', type: OrderType.Limit, side: OrderSide.Buy, amount: 2, price: 90000 };
    expect(await exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: 'inverse-01' })).toEqual({
      orderIds: ['4242'], clientOrderId: 'inverse-01',
    });
    const payload = JSON.parse(tape.writes[0].body!);
    expect(payload).toMatchObject({ category: 'inverse', request: [{ symbol: 'BTCUSD', qty: '2', orderLinkId: 'inverse-01' }] });
    expect(tape.socketUrls.some((url) => url.includes('/v5/public/inverse'))).toBe(true);
    await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'invalid.id' }])).rejects.toThrow('clientOrderId must match');
    expect(tape.writes).toHaveLength(1);
  });

  it('keeps inverse native sizing through the Rust frame and TypeScript store bridge', async () => {
    const tape = bybitTransport('inverse');
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('bybitinverse', { key: 'k', secret: 's' }, store);
    await exchange.start();
    await until(() => tape.sockets.has('/v5/private'));
    const privateSocket = tape.sockets.get('/v5/private')!;
    privateSocket(JSON.stringify({ topic: 'position.inverse', data: [{
      category: 'inverse', symbol: 'BTCUSD', positionIdx: 0, side: 'Buy', size: '1000',
      avgPrice: '80000', leverage: '10', unrealisedPnl: '0.002',
    }] }));
    privateSocket(JSON.stringify({ topic: 'order.inverse', data: [{
      category: 'inverse', symbol: 'BTCUSD', orderId: 'full-stop', side: 'Sell', orderStatus: 'Untriggered',
      orderType: 'Market', stopOrderType: 'StopLoss', tpslMode: 'Full', triggerPrice: '80000',
      qty: '11', leavesQty: '7', cumExecQty: '4', reduceOnly: true,
    }] }));
    await until(() => store.positions.length === 1 && store.orders.some((o) => o.id === 'full-stop'));
    expect(store.positions[0]).toMatchObject({ notional: 1000, extra: { contractsNative: 1000, unrealizedPnlNative: 0.002 } });
    expect(store.orders.find((o) => o.id === 'full-stop')).toMatchObject({
      amount: 0, filled: 4, extra: { remainingNative: 7, remainingNotional: 7 },
    });
  });

  it('starts spot without derivatives reads and sends one native order with a caller ID', async () => {
    const tape = bybitTransport('spot');
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('bybitspot', { key: 'k', secret: 's' }, store);
    await exchange.start();
    expect(exchange.isSpot).toBe(true);
    expect(store.marketsBySymbol.BTCUSDT).toMatchObject({
      isSpot: true, precision: { amount: 0.000001 }, limits: { leverage: { min: 1, max: 1 } },
    });
    expect(store.positions).toEqual([]);
    expect(tape.requests.some(({ url }) => /position|account\/info|fee-rate|PreLaunch/.test(url))).toBe(false);
    expect(tape.requests.filter(({ url }) => url.includes('instruments-info')).every(({ url }) => !url.includes('limit='))).toBe(true);
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.001, price: 90000 };
    expect(await exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: 'spot-01' })).toEqual({
      orderIds: ['4242'], clientOrderId: 'spot-01',
    });
    expect(tape.writes).toHaveLength(1);
    expect(tape.writes[0].path).toBe('/v5/order/create');
    expect(JSON.parse(tape.writes[0].body!)).toMatchObject({ category: 'spot', symbol: 'BTCUSDT', qty: '0.001', orderLinkId: 'spot-01' });
    await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'invalid.id' }])).rejects.toThrow('clientOrderId must match');
    expect(tape.writes).toHaveLength(1);
  });

  it.each([false, true])('selects the demo REST and socket environments (testnet=%s)', async (testnet) => {
    const tape = bybitTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange('bybitdemo', { key: 'k', secret: 's', testnet }, new DefaultStore());
    await exchange.start();
    const restHost = testnet ? 'api-testnet.bybit.com' : 'api-demo.bybit.com';
    expect(tape.requests.every(({ url }) => new URL(url).hostname === restHost)).toBe(true);
    await until(() => tape.socketUrls.some((url) => url.includes('/v5/private')));
    expect(new URL(tape.socketUrls.find((url) => url.includes('/v5/private'))!).hostname).toBe(
      testnet ? 'stream-testnet.bybit.com' : 'stream-demo.bybit.com',
    );
    expect(new URL(tape.socketUrls.find((url) => url.includes('/v5/public/linear'))!).hostname).toBe(
      testnet ? 'stream-testnet.bybit.com' : 'stream.bybit.com',
    );
    expect(exchange.isDemo).toBe(true);
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.001, price: 90000 };
    await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'invalid.id' }])).rejects.toThrow('clientOrderId must match');
    expect(tape.writes).toEqual([]);
  });

  it('rejects Hyro without configured origins before any transport I/O', async () => {
    const prod = { ...HYROTRADER_PROD_URL };
    const simulator = { ...HYROTRADER_SIMULATOR_URL };
    const tape = bybitTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    try {
      Object.assign(HYROTRADER_PROD_URL, { livenet: '', testnet: '' });
      Object.assign(HYROTRADER_SIMULATOR_URL, { livenet: '', testnet: '' });
      exchange = new RustEngineExchange('hyrotrader', { key: 'k', secret: 's' }, new DefaultStore());
      expect(exchange.mintClientOrderId({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 90000 })).toBeUndefined();
      await expect(exchange.start()).rejects.toThrow('URLs are not configured');
      expect(tape.requests).toEqual([]);
      expect(tape.socketUrls).toEqual([]);
    } finally {
      Object.assign(HYROTRADER_PROD_URL, prod);
      Object.assign(HYROTRADER_SIMULATOR_URL, simulator);
    }
  });

  it('forwards frames that rebuild the same store and book events on the main thread', async () => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const workerStore = new DefaultStore();
    exchange = new RustEngineExchange('binance', { key: '', secret: '', publicDataOnly: true }, workerStore);
    const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const mainBooks: unknown[] = [];
    main.emitter.on('listenOrderBook', (book: unknown) => mainBooks.push(book));
    // The worker transfers each frame; main gets its own buffer.
    exchange.onFrame = (frame) => main.applyWorkerFrame(frame.slice(0));
    const workerBooks: unknown[] = [];

    await exchange.start();
    exchange.listenOrderBook('BTCUSDT', (book) => workerBooks.push(book));
    await until(() => tape.sockets.has('/public/ws'));
    const publicSocket = tape.sockets.get('/public/ws')!;
    publicSocket('{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":90,"u":105,"pu":89,"b":[["98.5","1"]],"a":[]}');
    await until(() => tape.requests.some((r) => r.includes('/fapi/v1/depth')));
    publicSocket('{"e":"bookTicker","s":"BTCUSDT","b":"99.5","a":"99.6"}');
    await until(() => mainBooks.length > 0 && main.store.tickersBySymbol.BTCUSDT?.bid === 99.5);

    expect(main.store.markets).toEqual(workerStore.markets);
    expect(main.store.tickers).toEqual(workerStore.tickers);
    expect(main.store.loaded).toEqual(workerStore.loaded);
    const { symbol, orderbook } = mainBooks[0] as { symbol: string; orderbook: { bids: unknown[] } };
    expect(symbol).toBe('BTCUSDT');
    expect(orderbook.bids).toEqual([
      { price: 99, amount: 5, total: 5 },
      { price: 98.5, amount: 1, total: 6 },
    ]);
    expect(workerBooks).toEqual([]);
    main.dispose();
  });

  it('validates an account by its balance, or reports the venue error', async () => {
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => recordedTransport().transport);
    exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
    expect(await exchange.validateAccount()).toMatchObject({ error: '', balance: { total: 1000 } });
    exchange.dispose();

    const rejected = { status: 401, body: '{"code":-2015,"msg":"Invalid API-key, IP, or permissions for action."}' };
    setRustEngineTransport(() => recordedTransport({ account: rejected, leverageBracket: rejected }).transport);
    exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
    const result = await exchange.validateAccount();
    expect(result.error).toContain('Invalid API-key');
  });
});
