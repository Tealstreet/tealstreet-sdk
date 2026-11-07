import { vi } from 'vitest';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, OrderBook } from '../../types';

import { createHmac } from 'node:crypto';

import { Wallet, verifyTypedData } from 'ethers';
import { afterEach, expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { isRustVenue, RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

const NOW = 1_800_000_000_000;
const TOKEN = '123456789012345678901234567890123456789012345678901234567890123456789';
const NO_TOKEN = '123456789012345678901234567890123456789012345678901234567890123456790';
const SYMBOL = 'cafe-open-Yes';
// Public, deterministic secp256k1 test scalar; these are never live credentials.
const KEY = `0x${'0'.repeat(63)}1`;
const SIGNER = new Wallet(KEY).address;
const FUNDER = '0x1111111111111111111111111111111111111111';
const API = { apiKey: 'documented-fake-api-key', apiSecret: 'c2FmZS1mYWtlLXNlY3JldA==', apiPassphrase: 'documented-fake-passphrase' };
const cleanup: Array<() => void> = [];
afterEach(() => cleanup.splice(0).reverse().forEach((dispose) => dispose()));

type RecordedSocket = { url: string; sent: string[]; message(text: string): void };
type RequestHandler = (request: Parameters<Transport['request']>[0], sockets: RecordedSocket[]) => HttpResponse | undefined | Promise<HttpResponse | undefined>;
const response = (body: unknown): HttpResponse => ({ status: 200, body: new TextEncoder().encode(JSON.stringify(body)) });
const recorded = (handle?: RequestHandler) => {
  const requests: Array<Parameters<Transport['request']>[0]> = [];
  const sockets: RecordedSocket[] = [];
  const transport: Transport = {
    now: () => NOW,
    prepareHttp: prepareRecordedHttp,
    connect: (url, handlers) => {
      const sent: string[] = [];
      sockets.push({ url, sent, message: handlers.onMessage });
      queueMicrotask(() => handlers.onOpen());
      return { send: (text) => { sent.push(text); if (text === 'PING') queueMicrotask(() => handlers.onMessage('PONG')); }, close() {} };
    },
    request: async (request) => {
      requests.push(request);
      const handled = await handle?.(request, sockets);
      if (handled) return handled;
      const url = new URL(request.url);
      if (url.hostname === 'gamma-api.polymarket.com' && url.pathname === '/markets/keyset') return response({ markets: [{
        conditionId: '0xcondition', slug: 'cafe-open', question: 'Will café open?', description: 'Exact description', endDate: '2027-12-31T00:00:00Z',
        active: true, closed: false, enableOrderBook: true, acceptingOrders: true,
        clobTokenIds: JSON.stringify([TOKEN, NO_TOKEN]), outcomes: JSON.stringify(['Yes', 'No']),
        orderPriceMinTickSize: 0.001, orderMinSize: 5, negRisk: true,
      }] });
      if (url.pathname === '/midpoints') return response({ [TOKEN]: '0.6', [NO_TOKEN]: '0.4' });
      if (url.pathname === '/time') return response(NOW / 1000);
      if (url.pathname === '/auth/derive-api-key') return response({ apiKey: API.apiKey, secret: API.apiSecret, passphrase: API.apiPassphrase });
      if (url.pathname === '/balance-allowance') return response({ balance: '25000000', allowances: {} });
      if (url.pathname === '/data/orders') return response({ data: [], next_cursor: 'LTE=' });
      if (url.hostname === 'data-api.polymarket.com' && url.pathname === '/positions') return response([{
        asset: TOKEN, conditionId: '0xcondition', size: 25.5, avgPrice: 0.4, curPrice: 0.6,
        cashPnl: 5.1, realizedPnl: 1.2, redeemable: false, mergeable: true,
      }]);
      throw new Error(`Unexpected Polymarket request ${request.method} ${request.url}`);
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  return { transport, requests, sockets };
};
const until = async (check: () => boolean) => {
  for (let i = 0; i < 150; i++) {
    if (check()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error('Polymarket boundary condition did not settle');
};
const start = async (options: ExchangeOptions, handle?: RequestHandler) => {
  const tape = recorded(handle);
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape.transport);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange('polymarket', options, store);
  cleanup.push(() => exchange.dispose());
  const errors: unknown[] = [];
  exchange.on('error', (error: unknown) => errors.push(error));
  const startup = exchange.start();
  await Promise.race([
    startup,
    new Promise<never>((_resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`Polymarket startup: ${JSON.stringify({ errors, loaded: store.loaded, urls: tape.requests.map((r) => r.url) })}`)), 2500);
      void startup.finally(() => clearTimeout(timer)).catch(() => {});
    }),
  ]);
  return { ...tape, exchange, store };
};

it('loads outcomes as exact token markets and lazily streams complete CLOB books into worker and main frames', async () => {
  expect(isRustVenue('polymarket')).toBe(false);
  const { exchange, requests, sockets, store } = await start({ key: '', secret: '', publicDataOnly: true });
  expect(store.markets.map((market) => [market.id, market.symbol, market.quote])).toEqual([[TOKEN, SYMBOL, 'pUSD'], [NO_TOKEN, 'cafe-open-No', 'pUSD']]);
  expect(store.markets[0].info).toEqual({ tokenId: TOKEN, conditionId: '0xcondition', outcome: 'Yes', tickSize: 0.001, negRisk: true, question: 'Will café open?', description: 'Exact description', endDate: '2027-12-31T00:00:00Z', slug: 'cafe-open' });
  expect(store.tickersBySymbol[SYMBOL].last).toBe(0.6);
  expect(sockets).toEqual([]);
  expect(requests.every((request) => ['/markets/keyset', '/midpoints'].includes(new URL(request.url).pathname))).toBe(true);
  const main = new BaseExchange('polymarket', { key: '', secret: '' }, new DefaultStore());
  cleanup.push(() => main.dispose());
  main.applyWorkerFrame(exchange.snapshotFrame()!);
  expect(main.store.markets[0].info).toEqual(store.markets[0].info);
  const books: OrderBook[] = [];
  const unsubscribe = exchange.listenOrderBook(SYMBOL, (book) => books.push(book));
  cleanup.push(unsubscribe);
  await until(() => sockets.some((socket) => socket.sent.length > 0));
  const socket = sockets.find((socket) => socket.url.endsWith('/market'))!;
  expect(JSON.parse(socket.sent[0])).toMatchObject({ type: 'market', assets_ids: [TOKEN] });
  socket.message(JSON.stringify({ event_type: 'book', asset_id: TOKEN, timestamp: String(NOW), bids: [{ price: '0.55', size: '20' }, { price: '0.50', size: '30' }], asks: [{ price: '0.65', size: '10' }, { price: '0.70', size: '15' }] }));
  await until(() => books.length > 0);
  expect(books.at(-1)).toMatchObject({ bids: [{ price: 0.55, amount: 20 }, { price: 0.5, amount: 30 }], asks: [{ price: 0.65, amount: 10 }, { price: 0.7, amount: 15 }] });
  const mainBooks: Array<{ symbol: string; orderbook: OrderBook }> = [];
  main.on('listenOrderBook', (event: { symbol: string; orderbook: OrderBook }) => mainBooks.push(event));
  main.applyWorkerFrame(exchange.snapshotFrame()!);
  expect(mainBooks.at(-1)).toEqual({ symbol: SYMBOL, orderbook: books.at(-1) });
});

it.each([false, true])('authenticates with a wallet key and supplied=%s L2 credentials without a generic secret', async (supplied) => {
  const { exchange, requests, sockets, store } = await start({ key: KEY, secret: '', polymarket: supplied ? { ...API, signerType: 1, funderAddress: FUNDER } : undefined });
  expect(exchange.isPublicDataOnly).toBe(false);
  expect(store.balance.total).toBe(25);
  expect(store.positions[0]).toMatchObject({ symbol: SYMBOL, contracts: 25.5, leverage: 1, unrealizedPnl: 5.1, realizedPnl: 1.2, info: { tokenId: TOKEN, conditionId: '0xcondition', redeemable: false, mergeable: true } });
  expect(store.positions[0].notional).toBeCloseTo(15.3);
  const main = new BaseExchange('polymarket', { key: '', secret: '' }, new DefaultStore());
  cleanup.push(() => main.dispose());
  main.applyWorkerFrame(exchange.snapshotFrame()!);
  expect(main.store.positions[0]).toEqual(store.positions[0]);
  const positions = new URL(requests.find((request) => new URL(request.url).pathname === '/positions')!.url);
  expect(positions.searchParams.get('user')).toBe(supplied ? FUNDER : SIGNER);
  const balance = new URL(requests.find((request) => new URL(request.url).pathname === '/balance-allowance')!.url);
  expect(balance.searchParams.get('signature_type')).toBe(supplied ? '1' : '0');
  for (const request of requests.filter((request) => request.headers?.POLY_API_KEY)) {
    const headers = request.headers!;
    const expected = createHmac('sha256', Buffer.from(API.apiSecret, 'base64'))
      .update(`${headers.POLY_TIMESTAMP}${request.method}${new URL(request.url).pathname}${request.body ?? ''}`)
      .digest('base64').replace(/\+/g, '-').replace(/\//g, '_');
    expect(headers).toMatchObject({ POLY_ADDRESS: SIGNER, POLY_API_KEY: API.apiKey, POLY_PASSPHRASE: API.apiPassphrase, POLY_SIGNATURE: expected });
  }
  const derive = requests.find((request) => new URL(request.url).pathname === '/auth/derive-api-key');
  if (supplied) expect(derive).toBeUndefined();
  else {
    expect(derive).toBeDefined();
    const headers = derive!.headers!;
    expect(verifyTypedData({ name: 'ClobAuthDomain', version: '1', chainId: 137 }, {
      ClobAuth: [{ name: 'address', type: 'address' }, { name: 'timestamp', type: 'string' }, { name: 'nonce', type: 'uint256' }, { name: 'message', type: 'string' }],
    }, { address: SIGNER, timestamp: String(NOW / 1000), nonce: 0, message: 'This message attests that I control the given wallet' }, headers.POLY_SIGNATURE)).toBe(SIGNER);
    expect(headers).toMatchObject({ POLY_ADDRESS: SIGNER, POLY_TIMESTAMP: String(NOW / 1000), POLY_NONCE: '0' });
  }
  await until(() => sockets.some((socket) => socket.url.endsWith('/user') && socket.sent.length > 0));
  const user = sockets.find((socket) => socket.url.endsWith('/user'))!;
  expect(JSON.parse(user.sent[0])).toMatchObject({ type: 'user', auth: { apiKey: API.apiKey, secret: API.apiSecret, passphrase: API.apiPassphrase } });
  const writesBefore = requests.filter((request) => request.method !== 'GET').length;
  const regular = { symbol: SYMBOL, type: OrderType.Limit, side: OrderSide.Buy, price: 0.5, amount: 10 };
  await expect(exchange.placeOrders([regular, { ...regular, clientOrderId: 'unsupported-caller' }])).rejects.toThrow(/client.*id/i);
  expect(requests.filter((request) => request.method !== 'GET')).toHaveLength(writesBefore);
});

it.each([
  { key: 'bad-wallet' },
  { polymarket: { apiKey: API.apiKey } },
  { polymarket: { ...API, apiSecret: '' } },
  { polymarket: { signerType: 1 as const } },
  { polymarket: { signerType: 3 as const, funderAddress: 'bad-funder' } },
])('rejects invalid private configuration before any transport action: %o', async (override) => {
  const tape = recorded();
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape.transport);
  const exchange = new RustEngineExchange('polymarket', { key: KEY, secret: '', ...override }, new DefaultStore());
  cleanup.push(() => exchange.dispose());
  await expect(exchange.start()).rejects.toThrow(/Polymarket/);
  expect(tape.requests).toEqual([]);
  expect(tape.sockets).toEqual([]);
});

const limitOrder = () => ({ symbol: SYMBOL, type: OrderType.Limit, side: OrderSide.Buy, price: 0.5, amount: 10 });
const orderDetails = (id: string, status: string, filled: number) => ({
  id, asset_id: TOKEN, side: 'BUY', price: '0.5', original_size: '10', size_matched: String(filled), status, order_type: 'GTC', created_at: String(NOW / 1000),
});
const placementAck = (id: string, status = 'live') => ({ success: true, orderID: id, errorMsg: '', status });

it('returns partial batch identity through the actual Wasm host SDK Error after an ambiguous second write', async () => {
  let placements = 0;
  const transportSecret = 'audit-partial-write-secret';
  const { exchange, requests, store } = await start({ key: KEY, secret: '', polymarket: API }, (request) => {
    if (request.method !== 'POST' || new URL(request.url).pathname !== '/order') return;
    placements += 1;
    if (placements === 1) return response(placementAck('first-accepted'));
    throw new Error(`connection lost after sending the second order: ${API.apiKey} ${API.apiSecret} ${API.apiPassphrase} https://clob.polymarket.com/order?signature=${transportSecret}`);
  });
  const failure: unknown = await exchange.placeOrders([limitOrder(), limitOrder()]).then(
    () => { throw new Error('Expected the second order to fail'); },
    (error: unknown) => error,
  );
  expect(failure).toBeInstanceOf(Error);
  expect(failure).toMatchObject({
    message: 'HTTP transport request failed', partialOrderIds: ['first-accepted'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1,
  });
  const diagnostic = `${String(failure)}\n${JSON.stringify(failure)}`;
  for (const secret of [...Object.values(API), transportSecret, 'https://']) expect(diagnostic).not.toContain(secret);
  expect(requests.filter((request) => request.method === 'POST' && new URL(request.url).pathname === '/order')).toHaveLength(2);
  // This public SDK read explicitly flushes the worker frame, making the assertion independent of interval timing.
  await exchange.fetchLeverageForSymbol(SYMBOL);
  expect(store.orders.find((order) => order.id === 'first-accepted')).toMatchObject({ symbol: SYMBOL, amount: 10, remaining: 10 });
  const main = new BaseExchange('polymarket', { key: '', secret: '' }, new DefaultStore());
  cleanup.push(() => main.dispose());
  main.applyWorkerFrame(exchange.snapshotFrame()!);
  expect(main.store.orders).toEqual(expect.arrayContaining([expect.objectContaining({ id: 'first-accepted', amount: 10, remaining: 10 })]));
});

it('preserves a partial GTC remainder when a user update precedes a matched placement acknowledgement', async () => {
  let placed = false;
  const partial = orderDetails('partially-matched', 'LIVE', 3);
  const { exchange, store, sockets, requests } = await start({ key: KEY, secret: '', polymarket: API }, (request, connections) => {
    const path = new URL(request.url).pathname;
    if (request.method === 'POST' && path === '/order') {
      placed = true;
      const user = connections.find((socket) => socket.url.endsWith('/user'))!;
      user.message(JSON.stringify({ event_type: 'order', type: 'UPDATE', ...partial, timestamp: String(NOW + 1) }));
      return response(placementAck(partial.id, 'matched'));
    }
    if (path === '/data/orders' && placed) return response({ data: [partial], next_cursor: 'LTE=' });
  });
  await until(() => sockets.some((socket) => socket.url.endsWith('/user') && socket.sent.length > 0));
  const initialOrderReads = requests.filter((request) => new URL(request.url).pathname === '/data/orders').length;
  await expect(exchange.placeOrder(limitOrder())).resolves.toEqual([partial.id]);
  await until(() => requests.filter((request) => new URL(request.url).pathname === '/data/orders').length > initialOrderReads);
  await until(() => store.orders.some((order) => order.id === partial.id && order.remaining === 7));
  expect(store.orders.find((order) => order.id === partial.id)).toMatchObject({ amount: 10, filled: 3, remaining: 7, status: 'open' });
  const main = new BaseExchange('polymarket', { key: '', secret: '' }, new DefaultStore());
  cleanup.push(() => main.dispose());
  main.applyWorkerFrame(exchange.snapshotFrame()!);
  expect(main.store.orders.find((order) => order.id === partial.id)).toMatchObject({ filled: 3, remaining: 7 });
});

it('waits for coherent cancel fill details and floors a fractional replacement remainder through the actual SDK', async () => {
  const old = orderDetails('order-to-amend', 'LIVE', 2);
  let openOrders = [old];
  let details = 0;
  let replacements = 0;
  const { exchange, requests, store, sockets } = await start({ key: KEY, secret: '', polymarket: API }, (request, connections) => {
    const path = new URL(request.url).pathname;
    if (path === '/data/orders') return response({ data: openOrders, next_cursor: 'LTE=' });
    if (request.method === 'DELETE' && path === '/order') {
      expect(JSON.parse(request.body!)).toEqual({ orderID: old.id });
      openOrders = [];
      return response({ canceled: [old.id], not_canceled: {} });
    }
    if (request.method === 'GET' && path === `/data/order/${old.id}`) {
      details += 1;
      expect(replacements).toBe(0);
      if (details === 1) {
        const user = connections.find((socket) => socket.url.endsWith('/user'))!;
        user.message(JSON.stringify({ event_type: 'order', type: 'CANCELLATION', ...orderDetails(old.id, 'CANCELED', 4.994), timestamp: String(NOW + 1) }));
        return response(orderDetails(old.id, 'CANCELED', 2));
      }
      return response(orderDetails(old.id, 'CANCELED', 4.994));
    }
    if (request.method === 'POST' && path === '/order') {
      replacements += 1;
      expect(details).toBe(2);
      const body = JSON.parse(request.body!);
      expect(body).toMatchObject({ orderType: 'GTC', order: { tokenId: TOKEN, side: 'BUY', makerAmount: '3000000', takerAmount: '5000000' } });
      return response(placementAck('safe-replacement'));
    }
  });
  await until(() => sockets.some((socket) => socket.url.endsWith('/user') && socket.sent.length > 0));
  const original = store.orders.find((order) => order.id === old.id)!;
  expect(original).toMatchObject({ amount: 10, filled: 2, remaining: 8 });
  await expect(exchange.updateOrder({ order: original, update: { price: 0.6 } })).resolves.toEqual(['safe-replacement']);
  expect(details).toBe(2);
  expect(replacements).toBe(1);
  expect(requests.filter((request) => request.method === 'DELETE' && new URL(request.url).pathname === '/order')).toHaveLength(1);
  await exchange.fetchLeverageForSymbol(SYMBOL);
  expect(store.orders.find((order) => order.id === 'safe-replacement')).toMatchObject({ amount: 5, remaining: 5, price: 0.6 });
  const main = new BaseExchange('polymarket', { key: '', secret: '' }, new DefaultStore());
  cleanup.push(() => main.dispose());
  main.applyWorkerFrame(exchange.snapshotFrame()!);
  expect(main.store.orders).toEqual(expect.arrayContaining([expect.objectContaining({ id: 'safe-replacement', amount: 5, remaining: 5, price: 0.6 })]));
  expect(main.store.orders.some((order) => order.id === old.id)).toBe(false);
});

it('reports immutable one-times prediction-market leverage without any private request', async () => {
  const { exchange, requests, store } = await start({ key: '', secret: '', publicDataOnly: true, polymarket: { marketSlugs: ['cafe-open'] } });
  const requestCount = requests.length;
  await expect(exchange.fetchSymbolMaxLeverage(SYMBOL)).resolves.toBe(1);
  await expect(exchange.fetchLeverageForSymbol(SYMBOL)).resolves.toEqual({ buyLeverage: 1, sellLeverage: 1 });
  expect(store.fetchedLeverage[SYMBOL]).toEqual({ buyLeverage: 1, sellLeverage: 1 });
  expect(requests).toHaveLength(requestCount);
  const discovery = new URL(requests.find((request) => new URL(request.url).pathname === '/markets/keyset')!.url);
  expect(discovery.searchParams.getAll('slug')).toEqual(['cafe-open']);
});

it('cancels every venue order through native cancel-all even with an empty SDK order cache', async () => {
  const { exchange, requests, store } = await start({ key: KEY, secret: '', polymarket: API }, (request) => {
    if (new URL(request.url).pathname !== '/cancel-all') return;
    expect(request.method).toBe('DELETE');
    expect(request.body).toBeUndefined();
    const headers = request.headers!;
    const signature = createHmac('sha256', Buffer.from(API.apiSecret, 'base64'))
      .update(`${headers.POLY_TIMESTAMP}DELETE/cancel-all`)
      .digest('base64').replace(/\+/g, '-').replace(/\//g, '_');
    expect(headers.POLY_SIGNATURE).toBe(signature);
    return response({ canceled: ['order-never-seen-in-sdk-cache'], not_canceled: {} });
  });
  expect(store.orders).toEqual([]);
  await expect(exchange.cancelAllOrders()).resolves.toBeUndefined();
  expect(requests.filter((request) => request.method === 'DELETE').map((request) => new URL(request.url).pathname)).toEqual(['/cancel-all']);
});

it('queries the signer account and a full REST book without starting a book subscription', async () => {
  const { exchange, sockets } = await start({ key: KEY, secret: '', polymarket: API }, (request) => {
    if (new URL(request.url).pathname !== '/book') return;
    expect(new URL(request.url).searchParams.get('token_id')).toBe(TOKEN);
    return response({ asset_id: TOKEN, bids: [{ price: '0.4', size: '2' }, { price: '0.5', size: '3' }], asks: [{ price: '0.8', size: '5' }, { price: '0.6', size: '4' }] });
  });
  expect(await exchange.getAccount()).toEqual({ userId: SIGNER, affiliateId: null });
  expect(await exchange.fetchOrderBook(SYMBOL)).toEqual({
    bids: [{ price: 0.5, amount: 3, total: 3 }, { price: 0.4, amount: 2, total: 5 }],
    asks: [{ price: 0.6, amount: 4, total: 4 }, { price: 0.8, amount: 5, total: 9 }],
  });
  expect(sockets.some((socket) => socket.url.endsWith('/market'))).toBe(false);
  await expect(exchange.fetchOrderBook('unknown')).rejects.toThrow('Unknown');
});

it('keeps more than one hundred levels in a default REST book query', async () => {
  const bids = Array.from({ length: 150 }, (_, i) => ({ price: String((i + 1) / 1000), size: '1' }));
  const { exchange } = await start({ key: '', secret: '', publicDataOnly: true }, (request) => {
    if (new URL(request.url).pathname === '/book') return response({ asset_id: TOKEN, bids, asks: [] });
  });
  const book = await exchange.fetchOrderBook(SYMBOL);
  expect(book.bids).toHaveLength(150);
  expect(book.bids[0]).toEqual({ price: 0.15, amount: 1, total: 1 });
  expect(book.bids[149]).toEqual({ price: 0.001, amount: 1, total: 150 });
  expect((await exchange.fetchOrderBook(SYMBOL, 10)).bids).toHaveLength(10);
});

it('requires execution coverage and final shares before replacing a partially filled order', async () => {
  const old = { ...orderDetails('coverage-boundary', 'LIVE', 2), associate_trades: ['prior-fill'] };
  let openOrders = [old];
  let reads = 0;
  let replacements = 0;
  const { exchange, store, sockets } = await start({ key: KEY, secret: '', polymarket: API }, (request) => {
    const path = new URL(request.url).pathname;
    if (path === '/data/orders') return response({ data: openOrders, next_cursor: 'LTE=' });
    if (request.method === 'DELETE' && path === '/order') {
      openOrders = [];
      return response({ canceled: [old.id], not_canceled: {} });
    }
    if (path === `/data/order/${old.id}`) {
      reads += 1;
      expect(replacements).toBe(0);
      return response({ ...orderDetails(old.id, 'CANCELED', reads === 1 ? 2 : 3), associate_trades: reads === 1 ? ['prior-fill'] : ['prior-fill', 'new-fill'] });
    }
    if (request.method === 'POST' && path === '/order') {
      replacements += 1;
      expect(reads).toBe(2);
      expect(JSON.parse(request.body!).order.takerAmount).toBe('7000000');
      return response(placementAck('coverage-replacement'));
    }
  });
  await until(() => sockets.some((socket) => socket.url.endsWith('/user') && socket.sent.length > 0));
  sockets.find((socket) => socket.url.endsWith('/user'))!.message(JSON.stringify({ event_type: 'trade', id: 'new-fill', taker_order_id: old.id, asset_id: TOKEN, side: 'BUY', size: '1', price: '0.5', status: 'MATCHED', trader_side: 'TAKER', owner: API.apiKey }));
  await until(() => store.orders.find((order) => order.id === old.id)?.filled === 3);
  const order = store.orders.find((order) => order.id === old.id)!;
  await expect(exchange.updateOrder({ order, update: { price: 0.6 } })).resolves.toEqual(['coverage-replacement']);
  expect(replacements).toBe(1);
});
