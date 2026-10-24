import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, OrderBook, UpdateOrderOpts } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHmac } from 'node:crypto';

import { BaseExchange } from '../exchanges/base';
import { HYROTRADER_PROD_URL, HYROTRADER_SIMULATOR_URL } from '../exchanges/bybit/bybit.types';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { prepareRecordedHttp } from './support/managedHttp';
import { tealstreetTransport } from '../transport/tealstreet/tealstreetTransport';
import { PRIVATE_PROXY_JP } from '../transport/tealstreet/transportResolution';
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
    prepareHttp: prepareRecordedHttp,
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
    instType: 'SWAP',
    uly: 'BTC-USDT',
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
  const xperp = {
    ...instrument,
    instId: 'BTC-USD_UM_XPERP-310822',
    instType: 'FUTURES',
    ruleType: 'xperp',
    uly: 'BTC-USD',
    ctVal: '0.0001',
    settleCcy: 'USD',
    lotSz: '1',
    minSz: '1',
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
    prepareHttp: prepareRecordedHttp,
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
      const url = new URL(request.url);
      const name = url.pathname.split('/').pop()!;
      const futures = url.searchParams.get('instType') === 'FUTURES';
      // EEA public/account discovery and private snapshots are separate product domains.
      const data = !futures
        ? responses[name]
        : name === 'instruments'
          ? [xperp]
          : name === 'tickers'
            ? [
                {
                  instId: xperp.instId,
                  last: '',
                  open24h: '',
                  bidPx: '0',
                  askPx: '0',
                  volCcy24h: '0',
                  vol24h: '0',
                },
              ]
            : ['positions', 'orders-pending', 'orders-algo-pending'].includes(name)
              ? []
              : responses[name];
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

/** BingX swap envelopes from the documented private tape, with no live I/O. */
const bingxTransport = () => {
  const sockets = new Map<string, (text: string) => void>();
  const writes: Array<{ path: string; body?: string }> = [];
  let nextOrderId = 4242;
  const transport: Transport = {
    // BingX credentialed reads are managed; route preparation precedes signing.
    prepareHttp: prepareRecordedHttp,
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

  it('rejects malformed Kraken signing credentials before account startup can hang', async () => {
    setWasmEngineLoader(loadNodeEngine);
    const tape = recordedTransport();
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange('kraken', { key: 'fixture-key', secret: 'invalid base64!' }, new DefaultStore());
    await expect(exchange.start()).rejects.toThrow(/base64/);
    expect(tape.requests).toEqual([]);
  });

  it('skips private metadata refresh for an implicitly public-only MEXC shell', async () => {
    exchange = new RustEngineExchange('mexc', { key: '', secret: '' }, new DefaultStore());
    expect(exchange.isPublicDataOnly).toBe(true);
    await expect(exchange.updateSymbolMetadata('BTCUSDT')).resolves.toBeUndefined();
  });

  it('skips private WOO metadata for implicit public-only credentials', async () => {
    exchange = new RustEngineExchange('woo', { key: '', secret: '' }, new DefaultStore());
    await expect(exchange.updateSymbolMetadata('BTCUSDT')).resolves.toBeUndefined();
  });

  it('keeps WOO journal PnL on the order axis when fills have no PnL', async () => {
    setWasmEngineLoader(loadNodeEngine);
    exchange = new RustEngineExchange('woo', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    await exchange.loadCapabilities();
    expect(exchange.hasStarted).toBe(false);
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

  it('routes real engine HTTP and private sockets through the account provider', async () => {
    const tape = recordedOkxTransport();
    const outgoing: Array<Parameters<Transport['request']>[0]> = [];
    const sockets: string[] = [];
    let tokens = 0;
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => ({
      ...tape.transport,
      request: async (request) => {
        outgoing.push(request);
        return tape.transport.request(request);
      },
      connect: (url, handlers) => {
        sockets.push(url);
        return tape.transport.connect(url, handlers);
      },
    }));
    exchange = new RustEngineExchange(
      'okx',
      {
        key: 'venue-key',
        secret: 'venue-secret',
        passphrase: 'venue-pass',
        tealstreetProxy: true,
        tealstreetTunnel: false,
        transport: {
          ...tealstreetTransport,
          rest: (context) => ({
            ...tealstreetTransport.rest(context),
            authorize: async (request) => {
              request.headers.set('Authorization', `Bearer fixture-${++tokens}`);
            },
          }),
        },
      },
      new DefaultStore(),
    );
    await exchange.start();
    expect(
      await exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 1,
        price: 39000,
      }),
    ).toEqual(['4242']);
    expect(outgoing.every((request) => request.url.startsWith(`${PRIVATE_PROXY_JP}/https://www.okx.com/`))).toBe(true);
    expect(outgoing.map((request) => request.headers.Authorization)).toEqual(
      outgoing.map((_, at) => `Bearer fixture-${at + 1}`),
    );
    const placement = outgoing.find((request) => request.url.endsWith('/trade/batch-orders'))!;
    expect(placement.headers['OK-ACCESS-SIGN']).toBeTruthy();
    expect(placement.body).toContain('39000');
    expect(sockets).toContain('wss://fsjnpy143pbz.asia.tealstreet.io/okx/private');
    expect(sockets).toContain('wss://fsjnpy143pbz.asia.tealstreet.io/okx/business');
  });

  it.each([
    ['binance', 4321, 4321],
    ['binance', 0, 0],
    ['binance', undefined, 20000],
    ['binancepm', 4321, 4321],
    ['binancepm', 0, 0],
    ['binancepm', undefined, 20000],
  ] as const)('signs secondary reads, primary writes and lookup for %s with recvWindow %s', async (venue, recvWindow, expectedWindow) => {
    const tape = recordedTransport({ balance: { status: 200, body: '[]' } });
    const accountHost = venue === 'binancepm' ? 'papi.binance.com' : 'fapi.binance.com';
    const orderPath = venue === 'binancepm' ? '/papi/v1/um/order' : '/fapi/v1/order';
    const outgoing: Array<Parameters<Transport['request']>[0]> = [];
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => ({
      ...tape.transport,
      request: async (request) => {
        outgoing.push(request);
        const url = new URL(request.url);
        if (request.method === 'GET' && url.pathname.endsWith('/order')) {
          return {
            status: 200,
            body: new TextEncoder().encode(
              JSON.stringify({
                orderId: 4243,
                symbol: 'BTCUSDT',
                clientOrderId: 'LostResponse1',
                status: 'FILLED',
                type: 'LIMIT',
                origType: 'LIMIT',
                side: 'BUY',
                positionSide: 'BOTH',
                reduceOnly: false,
                price: '90000',
                avgPrice: '90000',
                origQty: '0.012',
                executedQty: '0.012',
                time: 1700000000000,
                updateTime: 1700000000123,
                timeInForce: 'GTC',
              }),
            ),
          };
        }
        return tape.transport.request(request);
      },
    }));
    exchange = new RustEngineExchange(
      venue,
      {
        key: 'primary-key',
        secret: 'primary-secret',
        extra: { recvWindow },
        secondary_key: 'read-key',
        secondary_secret: 'read-secret',
        secondaryProxyUrl: 'https://read-proxy.example/egress',
        tealstreetProxy: false,
        tealstreetTunnel: false,
      },
      new DefaultStore(),
    );
    await exchange.start();
    const reads = outgoing.filter((r) => r.method === 'GET' && r.url.includes('signature='));
    expect(reads.length).toBeGreaterThan(0);
    for (const request of reads) {
      expect(request.url.startsWith(`https://read-proxy.example/egress/https://${accountHost}/`)).toBe(true);
      expect(request.headers['X-MBX-APIKEY']).toBe('read-key');
      expect(new URL(request.url).searchParams.getAll('recvWindow')).toEqual([String(expectedWindow)]);
      const [query, signature] = new URL(request.url).search.slice(1).split('&signature=');
      expect(signature).toBe(createHmac('sha256', 'read-secret').update(query).digest('hex'));
    }
    const publicRead = outgoing.find((r) => r.url.includes('/exchangeInfo'))!;
    expect(publicRead.url).toBe('https://fapi.binance.com/fapi/v1/exchangeInfo');
    expect(publicRead.headers['X-MBX-APIKEY']).toBeUndefined();
    const listenKey = outgoing.find((r) => r.method === 'POST' && r.url.endsWith('/listenKey'))!;
    expect(listenKey.headers['X-MBX-APIKEY']).toBe('primary-key');
    expect(listenKey.url).not.toContain('read-proxy');

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.012,
        price: 90000,
      }),
    ).resolves.toHaveLength(1);
    await expect(exchange.findOrderByClientOrderId('BTCUSDT', 'LostResponse1')).resolves.toMatchObject({
      found: true,
      order: { id: venue === 'binancepm' ? 'LostResponse1' : '4243', clientOrderId: 'LostResponse1', status: OrderStatus.Closed },
    });
    const primaryOrders = outgoing.filter((r) => new URL(r.url).pathname.endsWith('/order'));
    expect(primaryOrders.map((r) => r.method)).toEqual(['POST', 'GET']);
    for (const request of primaryOrders) {
      expect(request.url.startsWith(`https://${accountHost}${orderPath}?`)).toBe(true);
      expect(request.headers['X-MBX-APIKEY']).toBe('primary-key');
      expect(new URL(request.url).searchParams.getAll('recvWindow')).toEqual([String(expectedWindow)]);
      const [query, signature] = new URL(request.url).search.slice(1).split('&signature=');
      expect(signature).toBe(createHmac('sha256', 'primary-secret').update(query).digest('hex'));
    }
    expect(
      outgoing.filter((r) => r.method === 'GET' && r.url.includes('origClientOrderId=LostResponse1')),
    ).toHaveLength(1);
  });

  it.each([
    { secondary_key: 'read-key' },
    { secondary_secret: 'read-secret' },
    { secondary_key: '', secondary_secret: 'read-secret' },
  ])('rejects incomplete secondary signing keys before transport creation: %o', (secondary) => {
    let transports = 0;
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => {
      transports++;
      return recordedTransport().transport;
    });
    // The trusted setup boundary now rejects before BaseExchange can retain options.
    expect(() => new RustEngineExchange(
      'binance',
      { key: 'primary-key', secret: 'primary-secret', ...secondary },
      new DefaultStore(),
    )).toThrow('secondary signing keys require a nonempty key and secret');
    expect(transports).toBe(0);
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

  it('starts a remote-only private account through owned signing and retains streaming sessions', async () => {
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    const sign = vi.fn(async (work: Parameters<NonNullable<ExchangeOptions['remoteSigner']>['sign']>[0]) => ({
      id: work.id, wsAuthSignature: createHmac('sha256', 'delegated-provider-fixture').update(work.wsAuth!.toSign).digest('hex'),
    }));
    exchange = new RustEngineExchange('binance', { key: 'k', secret: '', remoteSigner: {
      accountName: 'test', sign, isAvailable: () => true, onAvailabilityChange: () => () => undefined,
    } }, store);
    await exchange.start();
    expect(store.balance).toMatchObject({ total: 1000, free: 900, used: 100 });
    expect(store.loaded.orders && store.loaded.positions).toBe(true);
    expect(sign).toHaveBeenCalled();
    const signedReads = tape.requests.filter((line) => line.includes('signature='));
    expect(signedReads.length).toBeGreaterThan(0);
    for (const [, url] of signedReads.map((line) => line.split(' '))) {
      const query = url.split('?')[1];
      const [payload, signature] = query.split('&signature=');
      expect(signature).toBe(createHmac('sha256', 'delegated-provider-fixture').update(payload).digest('hex'));
    }
    expect(tape.requests.some((request) => request.includes('listenKey'))).toBe(true);
    await until(() => [...tape.sockets.keys()].some((path) => path.includes('/private')));
    expect(exchange.options).not.toHaveProperty('secret');
    expect(exchange.options).not.toHaveProperty('remoteSigner');
  });

  it('keeps all three real Bitget public feeds direct on their first connection', async () => {
    const opened: Array<{ url: string; channel?: string; context?: { feed: string; recover: boolean } }> = [];
    const raw: Transport = {
      ...recordedTransport().transport,
      connect: (url, handlers) => {
        opened.push({ url, channel: handlers.channel, context: handlers.context });
        setTimeout(handlers.onOpen, 0);
        return { send() {}, close() {} };
      },
      request: async ({ url }) => {
        const path = new URL(url).pathname;
        const data = path.endsWith('/contracts')
          ? [{ symbol: 'BTCUSDT', baseCoin: 'BTC', quoteCoin: 'USDT', symbolStatus: 'normal', pricePlace: '1', priceEndStep: '1', sizeMultiplier: '0.001', minTradeNum: '0.001', maxOrderQty: '120', maxLever: '125' }]
          : [{ symbol: 'BTCUSDT', bidPr: '99.9', askPr: '100.1', lastPr: '100', indexPrice: '99', changeUtc24h: '0.01', baseVolume: '1000', quoteVolume: '100000', fundingRate: '0.0001' }];
        return { status: 200, body: new TextEncoder().encode(JSON.stringify({ code: '00000', data })) };
      },
    };
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => raw);
    exchange = new RustEngineExchange('bitget', {
      key: '', secret: '', publicDataOnly: true, tealstreetProxy: false, tealstreetTunnel: false,
      transport: tealstreetTransport,
    }, new DefaultStore());
    await exchange.start();
    await until(() => opened.length === 3);
    expect(opened.map(({ url }) => url)).toEqual(Array(3).fill('wss://ws.bitget.com/v2/ws/public'));
    expect(opened.every(({ channel, context }) => channel === 'public' && context?.recover === false)).toBe(true);
    expect(new Set(opened.map(({ context }) => context?.feed))).toEqual(new Set(['primary', 'secondary-1', 'secondary-2']));
  });

  it.each(['bitget', 'bitgetusdc'] as const)('rejects private %s without a nonempty passphrase', async (venue) => {
    const { WasmEngine } = await loadNodeEngine();
    for (const passphrase of [undefined, '']) {
      expect(() => new WasmEngine(venue, JSON.stringify({ key: 'k', secret: 's', passphrase }))).toThrow(
        'Invalid local signer setup',
      );
    }
  });

  it.each(['bitget', 'bitgetusdc'] as const)(
    'accepts %s with its passphrase or public-only credentials',
    async (venue) => {
      const { WasmEngine } = await loadNodeEngine();
      const options: Partial<ExchangeOptions>[] = [
        { key: 'k', secret: 's', passphrase: 'p' },
        { key: 'k', secret: 's', publicDataOnly: true },
        { key: 'k', secret: 's', passphrase: '', publicDataOnly: true },
        {},
        { key: 'k' },
        { secret: 's' },
      ];
      for (const value of options) {
        expect(() => new WasmEngine(venue, JSON.stringify(value))).not.toThrow();
      }
    },
  );

  it('starts a PayloadSigner account through actual signed REST with callback headers and no private session', async () => {
    const tape = recordedTransport();
    const baseRequest = tape.transport.request;
    const signed: Array<Parameters<Transport['request']>[0]> = [];
    tape.transport.request = async (request) => {
      if (request.url.includes('signature=')) signed.push(request);
      return baseRequest(request);
    };
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    const callback = vi.fn(async ({ payload }: { payload: string }) => ({
      signature: createHmac('sha256', 'delegated-provider-fixture').update(payload).digest('hex'),
      headers: { 'X-Proof': 'same-attempt' },
    }));
    exchange = new RustEngineExchange('binance', { key: 'k', secret: '', payloadSigner: callback }, store);
    await exchange.start();
    expect(store.balance).toMatchObject({ total: 1000, free: 900, used: 100 });
    expect(store.loaded.orders && store.loaded.positions).toBe(true);
    expect(callback).toHaveBeenCalled();
    expect(signed.length).toBeGreaterThan(0);
    for (const request of signed) {
      expect(request.headers['X-Proof']).toBe('same-attempt');
      const [payload, signature] = request.url.split('?')[1].split('&signature=');
      expect(signature).toBe(createHmac('sha256', 'delegated-provider-fixture').update(payload).digest('hex'));
    }
    expect(tape.requests.some((request) => request.includes('listenKey'))).toBe(false);
    expect([...tape.sockets.keys()].some((path) => path.includes('/private'))).toBe(false);
    expect(exchange.options).not.toHaveProperty('payloadSigner');
    expect(exchange.options).not.toHaveProperty('secret');
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
      const store = new DefaultStore();
      exchange = new RustEngineExchange(venue, { key: 'k', secret: 's', passphrase: 'p' }, store);
      await exchange.start();
      const eea = venue === 'okxeu' || venue === 'okxeudemo';
      expect(store.markets.map((m) => m.id)).toEqual(
        eea ? ['BTC-USDT-SWAP', 'BTC-USD_UM_XPERP-310822'] : ['BTC-USDT-SWAP'],
      );
      expect(
        tape.requests
          .filter((r) => new URL(r.url).pathname.endsWith('/instruments'))
          .map((r) => new URL(r.url).searchParams.get('instType')),
      ).toEqual(eea ? ['SWAP', 'FUTURES'] : ['SWAP']);
      for (const endpoint of ['positions', 'orders-pending']) {
        // Socket authentication can refresh the snapshot more than once.
        const scopes = tape.requests
          .filter((r) => new URL(r.url).pathname.endsWith(`/${endpoint}`))
          .map((r) => new URL(r.url).searchParams.get('instType'));
        expect([...new Set(scopes)]).toEqual(eea ? ['SWAP', 'FUTURES'] : ['SWAP']);
      }
      if (eea) {
        expect(store.tickersBySymbol.BTCUSD_UM_XPERP310822).toMatchObject({ last: 0, bid: 0, ask: 0, percentage: 0 });
      }
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

  it.each(['cryptocom', 'weex', 'bitget', 'bitgetusdc'] as const)('honors %s caller IDs through the inherited shell helpers', async (venue) => {
    // Reuse the existing wasm fixture to seed the shell's market preflight.
    const tape = recordedTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('binance', { key: '', secret: '', publicDataOnly: true }, store);
    await exchange.start();
    const market = store.markets[0];
    exchange.dispose();
    const symbol = venue === 'weex' ? 'BTCUSDT' : 'BTCUSD-PERP';
    store.update({ markets: [{ ...market, id: symbol, symbol }] });
    exchange = new RustEngineExchange(venue, { key: 'k', secret: 's', passphrase: 'fixture-passphrase' }, store);
    const opts = {
      symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.012,
      price: 90000,
      clientOrderId: 'Caller.crypto:01',
    };
    const sent: string[] = [];
    exchange.placeOrder = async (order) => {
      sent.push(order.clientOrderId!);
      return ['crypto-4242'];
    };
    // The inherited lookup reads open orders from native state; this shell never starts.
    exchange.fetchOrders = async () => store.orders;
    await expect(exchange.placeOrderWithClientOrderId(opts)).resolves.toEqual({
      orderIds: ['crypto-4242'],
      clientOrderId: opts.clientOrderId,
    });
    expect(sent).toEqual([opts.clientOrderId]);
    expect(exchange.mintClientOrderId(opts)).toMatch(/^ts[A-Za-z0-9]{1,34}$/);
    store.addOrder({
      ...opts,
      id: 'crypto-4242',
      status: OrderStatus.Open,
      filled: 0,
      remaining: opts.amount,
      reduceOnly: false,
      lastUpdate: 1,
    });
    await expect(exchange.findOrderByClientOrderId(opts.symbol, opts.clientOrderId)).resolves.toMatchObject({
      found: true,
      order: { id: 'crypto-4242', clientOrderId: opts.clientOrderId },
    });
    await expect(exchange.findOrderByClientOrderId(opts.symbol, 'missing')).resolves.toEqual({
      found: false,
      definitive: false,
    });
    if (venue === 'bitget' || venue === 'bitgetusdc') {
      await exchange.loadCapabilities();
      expect(exchange.hasStarted).toBe(false);
      for (const invalid of ['', 'invalid@id', 'a'.repeat(33)]) {
        await expect(exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId: invalid })).rejects.toThrow(
          'clientOrderId must match',
        );
        await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: invalid }])).rejects.toThrow(
          'clientOrderId must match',
        );
      }
      await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'bracket', stopLoss: 80000 }])).rejects.toThrow(
        'order shape',
      );
      await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: 'split', amount: 1001 }])).rejects.toThrow(
        'max size',
      );
      await expect(exchange.placeOrders([opts, opts])).rejects.toThrow('duplicated in one batch');
      expect(exchange.mintClientOrderId({ ...opts, stopLoss: 80000 })).toBeUndefined();
      expect(exchange.mintClientOrderId({ ...opts, amount: 1001 })).toBeUndefined();
      expect(sent).toEqual([opts.clientOrderId]);
    }
  });

  it.each(['bytes', 'legacy'] as const)(
    'returns no outbound action payload for ticker updates and preserves socket errors (%s)',
    async (entry) => {
      const tape = bybitTransport();
      const { WasmEngine } = await loadNodeEngine();
      const outputs: Array<string | undefined> = [];
      setWasmEngineLoader(async () => ({
        WasmEngine: class extends WasmEngine {
          constructor(venue: string, options: string) {
            super(venue, options);
            if (entry === 'legacy') {
              // An older engine exposes only the original string entry point.
              Object.defineProperty(this, 'wsMessageBytes', { value: undefined });
              Object.defineProperty(this, 'wsMessageActions', { value: undefined });
            }
          }
          wsMessage(socket: number, text: string, now: number) {
            const output = super.wsMessage(socket, text, now);
            outputs.push(output);
            return output;
          }
          wsMessageBytes(socket: number, bytes: Uint8Array, now: number) {
            const output = super.wsMessageBytes!(socket, bytes, now);
            outputs.push(output);
            return output;
          }
        },
      }));
      setRustEngineTransport(() => tape.transport);
      const store = new DefaultStore();
      exchange = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, store);
      const errors: string[] = [];
      exchange.emitter.on('error', (message: string) => errors.push(message));
      await exchange.start();
      await until(() => tape.sockets.has('/v5/public/linear'));
      const socket = tape.sockets.get('/v5/public/linear')!;
      socket(
        JSON.stringify({ topic: 'tickers.BTCUSDT', type: 'delta', data: { symbol: 'BTCUSDT', lastPrice: '90001' } }),
      );
      expect(outputs).toEqual([entry === 'bytes' ? undefined : '']);
      await until(() => store.tickersBySymbol.BTCUSDT.last === 90001);
      socket(JSON.stringify({ success: false, op: 'subscribe', ret_msg: 'topic rejected' }));
      expect(JSON.parse(outputs[1]!)).toEqual([
        { op: 'event', level: 'error', code: 'subscription.rejected', message: '[WS]subscribe:topic rejected' },
      ]);
      expect(errors).toEqual([expect.stringContaining('[WS]subscribe:topic rejected')]);
    },
  );

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

  it('applies a filled order before its fill reaches listeners at the book cadence', async () => {
    const tape = bybitTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, store);
    await exchange.start();
    await until(() => tape.sockets.has('/v5/private'));
    const row = { orderId: '4242', symbol: 'BTCUSDT', orderType: 'Limit', side: 'Buy', price: '90000', qty: '0.012' };
    const send = (message: unknown) => tape.sockets.get('/v5/private')!(JSON.stringify(message));
    send({ topic: 'order', data: [{ ...row, orderStatus: 'New', leavesQty: '0.012', cumExecQty: '0' }] });
    await until(() => store.orders.some((order) => order.id === '4242'));
    const seen: Array<{ status?: OrderStatus; filled?: number }> = [];
    exchange.on('fill', () => {
      const order = store.orders.find((candidate) => candidate.id === '4242');
      seen.push({ status: order?.status, filled: order?.filled });
    });
    // Bybit reports the order and its execution on separate private topics.
    send({
      topic: 'order',
      data: [
        { ...row, orderStatus: 'Filled', leavesQty: '0', cumExecQty: '0.012', cumExecValue: '1080', avgPrice: '90000' },
      ],
    });
    send({ topic: 'execution', data: [{ ...row, execId: 'e-1', execType: 'Trade', execQty: '0.012', execPrice: '90000' }] });
    await until(() => seen.length > 0);
    expect(seen[0]).toEqual({ status: OrderStatus.Closed, filled: 0.012 });
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
    expect(tape.requests.filter(({ method, url }) => method === 'GET' && new URL(url).pathname === '/v5/account/info')).toHaveLength(1);
    expect(tape.requests.some(({ url }) => /position|fee-rate|PreLaunch/.test(url))).toBe(false);
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

  it.each([OrderType.StopLoss, OrderType.TakeProfit])(
    'rejects a whole spot batch when its final %s has no trigger through the WASM boundary',
    async (type) => {
      const tape = bybitTransport('spot');
      setWasmEngineLoader(loadNodeEngine);
      setRustEngineTransport(() => tape.transport);
      exchange = new RustEngineExchange('bybitspot', { key: 'k', secret: 's' }, new DefaultStore());
      await exchange.start();
      const requestsBefore = tape.requests.length;
      const valid = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.001, price: 90000 };
      await expect(exchange.placeOrders([valid, { ...valid, type, price: undefined }])).rejects.toThrow(
        'finite positive trigger price',
      );
      expect(tape.requests).toHaveLength(requestsBefore);
      expect(tape.writes).toEqual([]);
    },
  );

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

  it.each(['amount', 'price', 'triggerPrice', 'stopLoss', 'takeProfit'] as const)(
    'rejects non-finite Bybit order fields before any batch write (%s)',
    async (field) => {
      const tape = bybitTransport('spot');
      setWasmEngineLoader(loadNodeEngine);
      setRustEngineTransport(() => tape.transport);
      exchange = new RustEngineExchange('bybitspot', { key: 'k', secret: 's' }, new DefaultStore());
      await exchange.start();
      const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.001, price: 90000 };
      for (const value of [NaN, Infinity, -Infinity]) {
        await expect(exchange.placeOrders([opts, { ...opts, [field]: value }])).rejects.toThrow(
          `Bybit ${field} must be finite`,
        );
        expect(tape.writes).toEqual([]);
      }
    },
  );

  it.each([NaN, Infinity, -Infinity])(
    'rejects non-finite Bybit tick protection before any batch write (%s)',
    async (ticks) => {
      const tape = bybitTransport('spot');
      setWasmEngineLoader(loadNodeEngine);
      setRustEngineTransport(() => tape.transport);
      exchange = new RustEngineExchange('bybitspot', { key: 'k', secret: 's' }, new DefaultStore());
      await exchange.start();
      const opts = { symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 0.001 };
      const protectedOrder = { ...opts, bybitSlippageToleranceTicks: ticks };
      await expect(exchange.placeOrders([opts, protectedOrder])).rejects.toThrow(
        'bybitSlippageToleranceTicks must be finite',
      );
      expect(tape.writes).toEqual([]);
    },
  );

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
  it('runs KuCoin futures through bullet tokens with API-v2 passphrase and broker signing', async () => {
    const requests: Array<{ method: string; url: string; headers: Record<string, string>; body?: string }> = [];
    const topics: string[] = [];
    let token = 0;
    const transport: Transport = {
      now: () => Date.now(),
      connect: (url, handlers) => {
        expect(url).toMatch(/^wss:\/\/kucoin.test\/socket\?token=token-/);
        setTimeout(() => { handlers.onOpen(); handlers.onMessage(JSON.stringify({ type: 'welcome', id: 'fixture' })); }, 0);
        return { close: () => {}, send: (text) => {
          const message = JSON.parse(text) as { type: string; topic?: string; id?: string };
          expect(message.type).not.toBe('login');
          if (message.topic) topics.push(message.topic);
          if (message.type === 'ping') setTimeout(() => handlers.onMessage(JSON.stringify({ type: 'pong', id: message.id })), 0);
        } };
      },
      request: async (request) => {
        requests.push(request);
        const name = new URL(request.url).pathname.split('/').pop()!;
        const data: Record<string, unknown> = {
          active: [{ symbol: 'XBTUSDTM', baseCurrency: 'XBT', settleCurrency: 'USDT', status: 'Open', lotSize: 1, tickSize: 0.1, multiplier: 0.1, maxOrderQty: 1000, maxLeverage: 100 }],
          allTickers: [{ symbol: 'XBTUSDTM', price: '100', bestBidPrice: '99', bestAskPrice: '101' }],
          positions: [{ symbol: 'XBTUSDTM', currentQty: 10, avgEntryPrice: '90', markPrice: '100', leverage: '10', marginMode: 'CROSS', positionMode: 0, unrealisedPnl: '0.1' }],
          'account-overview': { accountEquity: '1000', availableBalance: '900', orderMargin: '100', unrealisedPNL: '0.1' },
          getPositionMode: { positionMode: 0 },
          getMarginMode: { marginMode: 'CROSS' },
          getCrossUserLeverage: { leverage: '10' },
          orders: request.method === 'POST' ? { orderId: 'native-order' } : { currentPage: 1, totalPage: 1, totalNum: 1, items: [{ id: 'existing-native', symbol: 'XBTUSDTM', side: 'buy', type: 'limit', status: 'open', price: '95', size: 7, filledSize: 3, remainSize: 4, createdAt: 1700000000000 }] },
          stopOrders: { currentPage: 1, totalPage: 1, totalNum: 0, items: [] },
          snapshot: { sequence: 100, bids: [[99, 3]], asks: [[101, 4]] },
        };
        const payload = name.startsWith('bullet-') ? { token: `token-${++token}`, instanceServers: [{ endpoint: 'wss://kucoin.test/socket', pingInterval: 18000, pingTimeout: 10000 }] } : data[name];
        if (payload === undefined) throw new Error(`Untaped KuCoin request ${request.url}`);
        return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify({ code: '200000', data: payload })) };
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    };
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => transport);
    const store = new DefaultStore();
    exchange = new RustEngineExchange('kucoin', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-passphrase', extra: { kucoinBroker: { partner: 'partner', brokerKey: 'broker-fixture', brokerName: 'tealstreet' } } }, store);
    const main = new BaseExchange('kucoin', { key: '', secret: '' }, new DefaultStore());
    const books: OrderBook[] = [];
    main.emitter.on('listenOrderBook', ({ orderbook }: { orderbook: OrderBook }) => books.push(orderbook));
    exchange.onFrame = (frame) => main.applyWorkerFrame(frame.slice(0));
    await exchange.start();
    await until(() => main.store.orders.length === 1 && main.store.positions.length === 1);
    expect(store.marketsBySymbol.BTCUSDT.contractSize).toBe(0.1);
    expect(store.positions[0].contracts).toBe(1);
    expect(store.positions[0].extra?.contractsNative).toBe(10);
    expect(main.store.positions[0]).toMatchObject({ contracts: 1, extra: { contractsNative: 10 } });
    expect(main.store.orders[0]).toMatchObject({ amount: 0.7, filled: 0.3, remaining: 0.4, extra: { remainingNative: 4, filledInPlacementUnits: 3 } });
    const { CrossTradeManager } = await import('../crossTrade/runtime/CrossTradeManager');
    const masterStore = new DefaultStore();
    masterStore.update({ markets: main.store.markets, tickers: main.store.tickers });
    const closes: number[] = [];
    const copier = new CrossTradeManager({ getExchange: () => undefined, placeOrder: async (_copier, _market, _price, _account, _side, amount) => { closes.push(amount); } });
    await (copier as any)._auditCopierPositions({ masterWeight: 1 }, { account: 'copier', weight: 1, symbolMappings: {} }, { name: 'binance', store: masterStore }, main);
    expect(closes).toEqual([10]);
    expect(store.orders[0]).toMatchObject({ amount: 0.7, filled: 0.3, remaining: 0.4, extra: { remainingNative: 4, filledInPlacementUnits: 3 } });
    const ids = await exchange.placeOrder({ symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, price: 95, amount: 2, clientOrderId: 'caller-id' });
    expect(ids).toEqual(['native-order']);
    const write = requests.find((request) => request.method === 'POST' && request.url.endsWith('/orders'))!;
    expect(JSON.parse(write.body!)).toMatchObject({ clientOid: 'caller-id', size: '2', marginMode: 'CROSS', leverage: 10 });
    expect(write.headers['KC-API-KEY-VERSION']).toBe('2');
    expect(write.headers['KC-API-PASSPHRASE']).toBe(createHmac('sha256', 'fixture-secret').update('fixture-passphrase').digest('base64'));
    expect(write.headers['KC-API-PARTNER-SIGN']).toBe(createHmac('sha256', 'broker-fixture').update(write.headers['KC-API-TIMESTAMP'] + 'partner' + 'fixture-key').digest('base64'));
    await until(() => topics.includes('/contractMarket/tradeOrders'));
    exchange.listenOrderBook('BTCUSDT', () => {});
    await until(() => books.length > 0);
    expect(books[0].bids[0].amount).toBe(3);
    main.dispose();
  });

  it('runs Backpack perpetuals and preserves canonical caller IDs through placement and frames', async () => {
    const sockets: Array<(text: string) => void> = [];
    const correctedTime = 1765805280000;
    expect(Date.now()).not.toBe(correctedTime);
    // An explicit host/tape clock owns REST and WS signing, independently of SDK time.
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(correctedTime + 60_000);
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
      prepareHttp: prepareRecordedHttp,
      now: () => correctedTime,
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
      prepareHttp(...args: Parameters<NonNullable<Transport['prepareHttp']>>) {
        return this.#inner.prepareHttp!(...args);
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
    expect(exchange.mintClientOrderId({ ...opts, close: true })).toBeUndefined();
    await expect(exchange.placeOrderWithClientOrderId({ ...opts, close: true })).rejects.toThrow('order shape');
    await expect(exchange.placeOrders([opts, { ...opts, clientOrderId: '2', close: true }])).rejects.toThrow(
      'order shape',
    );
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
    'Invalid local signer setup',
  );
});
