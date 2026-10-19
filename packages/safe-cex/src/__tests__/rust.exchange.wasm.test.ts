import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, OrderBook, UpdateOrderOpts } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../exchanges/base';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../types';
import { virtualClock } from '../utils/virtualClock';

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

const recordedOkxTransport = () => {
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  const instrument = {
    instId: 'BTC-USDT-SWAP',
    ctType: 'linear',
    ctVal: '0.01',
    ctValCcy: 'BTC',
    settleCcy: 'USDT',
    state: 'live',
    lotSz: '0.01',
    tickSz: '0.1',
    minSz: '0.01',
    lever: '100',
    maxIcebergSz: '1000',
    maxLmtSz: '1000',
    maxMktSz: '1000',
    maxStopSz: '1000',
    maxTriggerSz: '1000',
    maxTwapSz: '1000',
  };
  const responses: Record<string, unknown[]> = {
    instruments: [instrument],
    tickers: [
      {
        instId: instrument.instId,
        last: '40000',
        open24h: '40000',
        bidPx: '39999.9',
        askPx: '40000.1',
        volCcy24h: '100',
        vol24h: '10000',
      },
    ],
    config: [{ posMode: 'net_mode', acctLv: '2' }],
    'account-position-risk': [{ balData: [{ ccy: 'USDT', eq: '5000' }] }],
    positions: [],
    'orders-pending': [
      {
        instId: instrument.instId,
        ordId: '301',
        clOrdId: 'Resting301',
        ordType: 'limit',
        state: 'live',
        side: 'buy',
        sz: '2',
        accFillSz: '0',
        px: '38000',
        lever: '10',
        reduceOnly: 'false',
      },
    ],
    'orders-algo-pending': [],
    'batch-orders': [{ ordId: '4242', sCode: '0', sMsg: '' }],
    'amend-order': [{ ordId: '301', sCode: '0', sMsg: '' }],
  };
  const transport: Transport = {
    now: () => Date.now(),
    connect: (_url, { onOpen, onMessage }) => {
      let closed = false;
      setTimeout(() => {
        if (!closed) onOpen();
      }, 0);
      return {
        send: (text) => {
          const reply =
            text === 'ping' ? 'pong' : JSON.parse(text).op === 'login' ? '{"event":"login","code":"0"}' : undefined;
          if (reply)
            setTimeout(() => {
              if (!closed) onMessage(reply);
            }, 0);
        },
        close: () => {
          closed = true;
        },
      };
    },
    request: async (request) => {
      requests.push(request);
      const name = new URL(request.url).pathname.split('/').pop()!;
      const data = responses[name];
      return {
        status: data ? 200 : 404,
        headers: '',
        body: new TextEncoder().encode(JSON.stringify({ code: '0', msg: '', data: data ?? [] })),
      };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, requests };
};

/** Bybit V5 envelopes for one linear market and an empty unified account. */
const bybitTransport = () => {
  const sockets = new Map<string, (text: string) => void>();
  const writes: Array<{ path: string; body?: string }> = [];
  const list = (rows: unknown[]) => ({ list: rows, nextPageCursor: '' });
  const transport: Transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      sockets.set(new URL(url).pathname, onMessage);
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url, body }) => {
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
                    symbol: 'BTCUSDT',
                    status: 'Trading',
                    baseCoin: 'BTC',
                    quoteCoin: 'USDT',
                    priceFilter: { tickSize: '0.10' },
                    lotSizeFilter: { qtyStep: '0.001', minOrderQty: '0.001', maxOrderQty: '120' },
                  },
                ],
          );
          break;
        case '/v5/market/tickers':
          result = list([{ symbol: 'BTCUSDT', lastPrice: '90000', bid1Price: '89999.9', ask1Price: '90000.1' }]);
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
  return { transport, sockets, writes };
};

/** BingX swap envelopes from the documented private tape, with no live I/O. */
const bingxTransport = () => {
  const sockets = new Map<string, (text: string) => void>();
  const writes: Array<{ path: string; body?: string }> = [];
  let nextOrderId = 4242;
  const transport: Transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      sockets.set(new URL(url).pathname, onMessage);
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url, body }) => {
      const path = new URL(url).pathname;
      // Listen-key POSTs are reads; keep trading-write assertions independent of startup.
      if (method !== 'GET' && path !== '/openApi/user/auth/userDataStream') writes.push({ path, body });
      let data: unknown;
      switch (path) {
        case '/openApi/swap/v2/quote/contracts':
          data = [
            {
              symbol: 'BTC-USDT',
              asset: 'BTC',
              currency: 'USDT',
              status: 1,
              quantityPrecision: 3,
              pricePrecision: 1,
              tradeMinLimit: '0.001',
              maxLongLeverage: 125,
            },
          ];
          break;
        case '/openApi/swap/v2/quote/ticker':
          data = [
            { symbol: 'BTC-USDT', lastPrice: '90000', priceChangePercent: '1', volume: '2', quoteVolume: '180000' },
          ];
          break;
        case '/openApi/swap/v2/quote/premiumIndex':
          data = [
            {
              symbol: 'BTC-USDT',
              markPrice: '90000',
              indexPrice: '89999',
              lastFundingRate: '0.01',
              nextFundingTime: 1760889600000,
            },
          ];
          break;
        case '/openApi/user/auth/userDataStream':
          return { status: 200, body: new TextEncoder().encode('{"listenKey":"documented-test-key"}') };
        case '/openApi/swap/v1/positionSide/dual':
          data = { dualSidePosition: 'false' };
          break;
        case '/openApi/swap/v3/user/balance':
          data = [{ asset: 'USDT', balance: '1000', availableMargin: '900', usedMargin: '100', unrealizedProfit: '0' }];
          break;
        case '/openApi/swap/v2/user/positions':
          data = [];
          break;
        case '/openApi/swap/v2/trade/openOrders':
          data = { orders: [] };
          break;
        case '/openApi/swap/v2/trade/order':
          data = {
            order: {
              orderId: String(nextOrderId++),
              symbol: 'BTC-USDT',
              side: 'BUY',
              type: 'LIMIT',
              origQty: '0.012',
              executedQty: '0',
              price: '90000',
              status: 'NEW',
              positionSide: 'BOTH',
              timeInForce: 'GTC',
              reduceOnly: false,
            },
          };
          break;
        default:
          throw new Error(`Unexpected BingX request: ${method} ${path}`);
      }
      return { status: 200, body: new TextEncoder().encode(JSON.stringify({ code: 0, msg: '', data })) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, sockets, writes };
};

const until = async (check: () => boolean) => {
  for (let i = 0; i < 200 && !check(); i++) await new Promise((resolve) => setTimeout(resolve, 10));
  expect(check()).toBe(true);
};

describe('RustEngineExchange', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => {
    exchange?.dispose();
    vi.restoreAllMocks();
  });

  it('skips private WOO metadata for implicit public-only credentials', async () => {
    exchange = new RustEngineExchange('woo', { key: '', secret: '' }, new DefaultStore());
    await expect(exchange.updateSymbolMetadata('BTCUSDT')).resolves.toBeUndefined();
  });

  it('keeps WOO journal PnL on the order axis when fills have no PnL', async () => {
    exchange = new RustEngineExchange('woo', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    const orders = vi.spyOn(exchange, 'fetchHistoricalOrders').mockResolvedValue([
      { id: 'open', symbol: 'ETHUSDT', side: OrderSide.Buy, type: OrderType.Limit, status: OrderStatus.Closed,
        price: 99, averagePrice: 100, amount: 1, filled: 1, reduceOnly: false,
        createdTime: 1_000_000, updatedTime: 1_000_000, realizedPnl: 0, fees: 0.1, feeAsset: 'USDT' },
      { id: 'close', symbol: 'ETHUSDT', side: OrderSide.Sell, type: OrderType.Limit, status: OrderStatus.Closed,
        price: 111, averagePrice: 110, amount: 1, filled: 1, reduceOnly: true,
        createdTime: 2_000_000, updatedTime: 2_000_000, realizedPnl: 10, fees: 0.1, feeAsset: 'USDT' },
    ]);
    const fills = vi.spyOn(exchange, 'fetchHistoricalFills').mockRejectedValue(new Error('fills lack PnL'));
    const records = await exchange.fetchHistoricalPositions({ account: 'woo-account', from: 500_000, to: 3_000_000 });
    expect(orders).toHaveBeenCalledWith({ from: 500_000, to: 3_000_000 });
    expect(fills).not.toHaveBeenCalled();
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({ account: 'woo-account', symbol: 'ETHUSDT', state: 'closed', realizedPnl: 10, sourceAxis: 'orders' });
  });

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

  it.each([
    { key: '', secret: '' },
    { key: 'k', secret: '' },
    { key: '', secret: 's' },
  ])('starts public data with incomplete credentials %o', async (options) => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('binance', options, store);

    await exchange.start();
    expect(store.loaded.markets && store.loaded.tickers).toBe(true);
    expect(tape.requests.some((request) => /account|listenKey|openOrders|leverageBracket/.test(request))).toBe(false);
    expect([...tape.sockets.keys()].some((path) => path.includes('/private'))).toBe(false);
  });

  it('rejects remote-only private signing before transport starts', async () => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange(
      'binance',
      {
        key: 'k',
        secret: '',
        remoteSigner: {
          accountName: 'test',
          sign: async () => ({ id: 'unused', error: 'must not be called' }),
          isAvailable: () => true,
          onAvailabilityChange: () => () => undefined,
        },
      },
      new DefaultStore(),
    );

    await expect(exchange.start()).rejects.toThrow('remote signing is unsupported');
    expect(tape.requests).toEqual([]);
  });

  it('rejects payload-only private signing before transport starts', async () => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange(
      'binance',
      {
        key: 'k',
        secret: '',
        payloadSigner: async () => ({ signature: 'unused' }),
      } satisfies ExchangeOptions,
      new DefaultStore(),
    );

    await expect(exchange.start()).rejects.toThrow('remote signing is unsupported');
    expect(tape.requests).toEqual([]);
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

  it.each(['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const)(
    'honors and mints OKX caller IDs through the SDK helpers on %s',
    async (venue) => {
      const tape = recordedOkxTransport();
      setWasmEngineLoader(loadNodeEngine);
      setRustEngineTransport(() => tape.transport);
      exchange = new RustEngineExchange(venue, { key: 'k', secret: 's', passphrase: 'p' }, new DefaultStore());
      await exchange.start();
      const order = {
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 2,
        price: 38500,
        clientOrderId: 'Caller123',
      };
      await expect(exchange.placeOrderWithClientOrderId(order)).resolves.toEqual({
        orderIds: ['4242'],
        clientOrderId: 'Caller123',
      });
      const writes = tape.requests.filter((request) => request.method === 'POST');
      expect(writes).toHaveLength(1);
      expect(JSON.parse(writes[0].body!)[0].clOrdId).toBe('Caller123');
      const minted = exchange.mintClientOrderId(order)!;
      expect(minted).toMatch(/^e847386590ce4dBC[A-Za-z0-9]{0,16}$/);
      await expect(exchange.findOrderByClientOrderId('BTCUSDT', 'Resting301')).resolves.toMatchObject({
        found: true,
        order: { id: '301' },
      });
    },
  );

  it('forwards combined OKX price and amount amendments to the native request', async () => {
    const tape = recordedOkxTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('okx', { key: 'k', secret: 's', passphrase: 'p' }, store);
    await exchange.start();
    const order = store.orders.find((o) => o.id === '301')!;
    await expect(exchange.updateOrder({ order, update: { price: 38600.1, amount: 3.2 } })).resolves.toEqual(['301']);
    const writes = tape.requests.filter((request) => request.method === 'POST');
    expect(writes).toHaveLength(1);
    expect(JSON.parse(writes[0].body!)).toEqual({
      instId: 'BTC-USDT-SWAP',
      ordId: '301',
      newPx: '38600.1',
      newSz: '3.2',
    });
    for (const update of [{}, { price: NaN }, { amount: Infinity }, { price: 38600.1, amount: NaN }]) {
      await expect(exchange.updateOrder({ order, update: update as UpdateOrderOpts['update'] })).rejects.toThrow(
        /requires|finite/,
      );
    }
    expect(tape.requests.filter((request) => request.method === 'POST')).toHaveLength(1);
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

  it('honors BingX minted and caller IDs through inherited helpers and finds the private-stream ID', async () => {
    const tape = bingxTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('bingx', { key: 'k', secret: 's' }, store);
    await exchange.start();
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.012, price: 90000 };
    const minted = exchange.mintClientOrderId(opts);
    expect(minted).toMatch(/^ts[a-z0-9]{0,21}$/);
    expect(await exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: minted! })).toEqual({
      orderIds: ['4242'],
      clientOrderId: minted,
    });
    const clientOrderId = 'caller_bingx-01';
    expect(await exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId })).toEqual({
      orderIds: ['4243'],
      clientOrderId,
    });
    expect(tape.writes).toHaveLength(2);
    expect(tape.writes.map(({ path }) => path)).toEqual([
      '/openApi/swap/v2/trade/order',
      '/openApi/swap/v2/trade/order',
    ]);
    expect(JSON.parse(tape.writes[0].body!).clientOrderId).toBe(minted);
    expect(JSON.parse(tape.writes[1].body!)).toMatchObject({ clientOrderId, symbol: 'BTC-USDT', quantity: 0.012 });
    await until(() => store.orders.some((order) => order.id === '4243'));
    expect(store.orders.find((order) => order.id === '4243')?.clientOrderId).toBeUndefined();
    await until(() => tape.sockets.has('/swap-market'));
    tape.sockets.get('/swap-market')!(
      JSON.stringify({
        e: 'ORDER_TRADE_UPDATE',
        E: Date.now(),
        o: {
          s: 'BTC-USDT',
          i: 4243,
          c: clientOrderId,
          S: 'BUY',
          o: 'LIMIT',
          X: 'NEW',
          q: '0.012',
          p: '90000',
          z: '0',
          l: '0',
          L: '0',
          ap: '0',
          f: 'GTC',
          ps: 'BOTH',
          R: false,
          wt: 'CONTRACT_PRICE',
        },
      }),
    );
    await until(() => store.orders.some((order) => order.clientOrderId === clientOrderId));
    expect(await exchange.findOrderByClientOrderId('BTCUSDT', clientOrderId)).toMatchObject({
      found: true,
      order: { id: '4243', clientOrderId, status: OrderStatus.Open },
    });
    expect(await exchange.findOrderByClientOrderId('BTCUSDT', 'missing')).toEqual({ found: false, definitive: false });
    await expect(
      exchange.updateOrder({
        order: store.orders.find((order) => order.id === '4243')!,
        update: { price: 90001, amount: 0.02 },
      }),
    ).rejects.toThrow('exactly one');
    expect(tape.writes).toHaveLength(2);
  });

  it('rejects invalid BingX caller IDs, brackets and mixed batches before trading writes', async () => {
    const tape = bingxTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange('bingx', { key: 'k', secret: 's' }, new DefaultStore());
    await exchange.start();
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.012, price: 90000 };
    for (const clientOrderId of ['', 'UPPER', 'a'.repeat(41)]) {
      await expect(exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId })).rejects.toThrow(
        '1-40 characters with no uppercase letters',
      );
    }
    await expect(
      exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: 'bracket', stopLoss: 80000 }),
    ).rejects.toThrow('order shape');
    expect(exchange.mintClientOrderId({ ...opts, stopLoss: 80000 })).toBeUndefined();
    expect(exchange.mintClientOrderId({ ...opts, close: true })).toBeUndefined();
    await expect(
      exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: 'close', close: true }),
    ).rejects.toThrow('order shape');
    await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'UPPER' }])).rejects.toThrow(
      '1-40 characters with no uppercase letters',
    );
    await expect(
      exchange.placeOrders([opts, { ...opts, clientOrderId: 'bracket', takeProfit: 100000 }]),
    ).rejects.toThrow('order shape');
    await expect(
      exchange.placeOrders([
        { ...opts, clientOrderId: 'same' },
        { ...opts, clientOrderId: 'same' },
      ]),
    ).rejects.toThrow('duplicated in one batch');
    expect(tape.writes).toEqual([]);
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

  it('runs Backpack perpetuals and preserves canonical caller IDs through placement and frames', async () => {
    const sockets: Array<(text: string) => void> = [];
    const correctedTime = 1765805280000;
    expect(Date.now()).not.toBe(correctedTime);
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(correctedTime);
    const signedTimestamps: string[] = [];
    const subscriptionTimestamps: string[] = [];
    const writes: Array<{ method: string; body?: string }> = [];
    const responses: Record<string, string> = {
      markets: '[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]',
      tickers:
        '[{"symbol":"SOL_USDC_PERP","lastPrice":"140","priceChangePercent":"0.01","volume":"10","quoteVolume":"1400"}]',
      markPrices: '[]',
      account: '{"leverageLimit":"10"}',
      position: '[]',
      orders: '[]',
      collateral: '{"netEquity":"1000","netEquityAvailable":"900","netEquityLocked":"100","pnlUnrealized":"0"}',
    };
    const transport: Transport = {
      now: () => Date.now(),
      connect: (_url, { onOpen, onMessage }) => {
        sockets.push(onMessage);
        setTimeout(onOpen, 0);
        return {
          send: (text) => {
            const message = JSON.parse(text);
            if (message.signature) subscriptionTimestamps.push(message.signature[2]);
          },
          close: () => {},
        };
      },
      request: async ({ method, url, body, headers }) => {
        if (headers['X-Timestamp']) signedTimestamps.push(headers['X-Timestamp']);
        if (method !== 'GET') writes.push({ method, body });
        const text = method === 'POST' ? '{"id":"800"}' : responses[new URL(url).pathname.split('/').pop()!];
        return { status: text ? 200 : 404, headers: '', body: new TextEncoder().encode(text ?? '{}') };
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    };
    setWasmEngineLoader(loadNodeEngine);
    class PrototypeTransport implements Transport {
      #inner: Transport;
      constructor(inner: Transport) {
        this.#inner = inner;
      }
      now() {
        return this.#inner.now();
      }
      connect(...args: Parameters<Transport['connect']>) {
        return this.#inner.connect(...args);
      }
      request(...args: Parameters<Transport['request']>) {
        return this.#inner.request(...args);
      }
      setTimeout(...args: Parameters<Transport['setTimeout']>) {
        return this.#inner.setTimeout(...args);
      }
      clearTimeout(...args: Parameters<Transport['clearTimeout']>) {
        return this.#inner.clearTimeout(...args);
      }
    }
    setRustEngineTransport(() => new PrototypeTransport(transport));
    const store = new DefaultStore();
    exchange = new RustEngineExchange(
      'backpack',
      { key: 'public-test-key', secret: Buffer.alloc(32, 7).toString('base64') },
      store,
    );
    await exchange.start();
    const opts = {
      symbol: 'SOL_USDC_PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 140,
      clientOrderId: '4294967295',
    };
    expect(await exchange.placeOrderWithClientOrderId(opts)).toEqual({
      orderIds: ['800'],
      clientOrderId: '4294967295',
    });
    expect(JSON.parse(writes[0].body!)).toMatchObject({ clientId: 4294967295, quantity: '1.00' });
    await expect(exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: '01' })).rejects.toThrow('uint32');
    expect(writes).toHaveLength(1);
    await until(() => sockets.length === 2 && subscriptionTimestamps.length === 1);
    expect(subscriptionTimestamps).toEqual([String(correctedTime)]);
    expect(signedTimestamps.length).toBeGreaterThan(0);
    expect(new Set(signedTimestamps)).toEqual(new Set([String(correctedTime)]));
    sockets[1](
      JSON.stringify({
        stream: 'account.orderUpdate',
        data: {
          e: 'orderAccepted',
          i: '800',
          c: 4294967295,
          E: Date.now() * 1000,
          X: 'New',
          s: opts.symbol,
          o: 'Limit',
          S: 'Bid',
          p: '140',
          q: '1',
          z: '0',
          f: 'GTC',
        },
      }),
    );
    await until(() => store.orders.some((o) => o.id === '800'));
    expect(store.orders[0].clientOrderId).toBe('4294967295');
    expect(await exchange.findOrderByClientOrderId(opts.symbol, '4294967295')).toMatchObject({
      found: true,
      order: { id: '800' },
    });
  });
});

it('WEEX private engines require the passphrase before starting I/O', async () => {
  const { WasmEngine } = await loadNodeEngine();
  expect(() => new WasmEngine('weex', JSON.stringify({ key: 'replay-key', secret: 'replay-secret' }))).toThrow(
    'WEEX requires a passphrase',
  );
});
