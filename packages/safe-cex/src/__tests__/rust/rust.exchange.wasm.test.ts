import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { OrderBook } from '../../types';

import { afterEach, describe, expect, it } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';

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
  account: '{"assets":[{"asset":"USDT","walletBalance":"1000","availableBalance":"900","initialMargin":"100","unrealizedProfit":"0"}]}',
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
