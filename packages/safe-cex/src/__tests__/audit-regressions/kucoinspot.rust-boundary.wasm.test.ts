import { vi } from 'vitest';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { Order, OrderFillEvent } from '../../types';

import { createHmac } from 'node:crypto';

import { expect, it } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { directTransport } from '../../transport/direct';
import { MarginType, OrderSide, OrderType } from '../../types';

it.each([false, true])('preserves KuCoin HF context, quantities and exact broker signatures with secondary reads=%s through the real wasm host', async (secondary) => {
  let now = 1_700_000_000_000;
  const requests: Array<Parameters<Transport['request']>[0]> = [];
  const sockets: string[] = [];
  const fills: OrderFillEvent[] = [];
  const errors: unknown[] = [];
  let privateSocket: Parameters<Transport['connect']>[1] | undefined;
  const hmac = (secret: string, text: string) => createHmac('sha256', secret).update(text).digest('base64');
  const ok = (data: unknown) => ({
    status: 200,
    body: new TextEncoder().encode(JSON.stringify({ code: '200000', data })),
  });
  const restOrder = {
    id: 'seed-order',
    symbol: 'BTC-USDT',
    clientOid: 'seed-caller',
    type: 'limit',
    side: 'buy',
    status: 'open',
    price: '90',
    size: '2',
    dealSize: '0.5',
    remainSize: '1.5',
    createdAt: now,
  };
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => ++now,
    connect: (url, handlers) => {
      sockets.push(url);
      if (url.includes('token=private-token')) privateSocket = handlers;
      queueMicrotask(() => {
        handlers.onOpen();
        handlers.onMessage('{"type":"welcome"}');
      });
      return { send() {}, close() {} };
    },
    request: async (request) => {
      requests.push(request);
      const url = new URL(request.url);
      expect(url.origin).toBe('https://api.kucoin.com');
      const path = url.pathname;
      if (path === '/api/v2/symbols')
        return ok([
          {
            symbol: 'BTC-USDT',
            baseCurrency: 'BTC',
            quoteCurrency: 'USDT',
            enableTrading: true,
            baseIncrement: '0.001',
            priceIncrement: '0.1',
            baseMinSize: '0.001',
            baseMaxSize: '100',
          },
        ]);
      if (path === '/api/v1/market/allTickers')
        return ok({ ticker: [{ symbol: 'BTC-USDT', last: '100', buy: '99', sell: '101' }] });
      if (path.startsWith('/api/v1/bullet-'))
        return ok({
          token: path.endsWith('private') ? 'private-token' : 'public-token',
          instanceServers: [{ endpoint: 'wss://spot.example.test/endpoint', pingInterval: 18000, pingTimeout: 10000 }],
        });
      if (path === '/api/v1/accounts')
        return ok([{ currency: 'USDT', available: '900', holds: '100', balance: '1000' }]);
      if (path === '/api/v1/hf/orders/active/symbols') return ok({ symbols: ['BTC-USDT'] });
      if (path === '/api/v1/hf/orders/active/page') return ok({ totalPage: 1, items: [restOrder] });
      if (path === '/api/v2/user-info')
        return ok({ userId: 'main-id', subUserId: 'sub-id', referrerUserId: 'broker-id' });
      if (path === '/api/v1/hf/fills')
        return ok({
          items: [
            {
              symbol: 'BTC-USDT',
              orderId: 'seed-order',
              side: 'buy',
              price: '89.5',
              size: '0.2',
              fee: '0.01',
              feeCurrency: 'USDT',
              createdAt: 1700000001000,
            },
          ],
        });
      if (request.method === 'POST' && path === '/api/v1/hf/orders') return ok({ orderId: 'placed-id' });
      if (request.method === 'POST' && path === '/api/v1/hf/orders/alter') return ok({ newOrderId: 'uncached-id' });
      if (request.method === 'DELETE' && path === '/api/v1/hf/orders/uncached-id')
        return ok({ orderId: 'uncached-id' });
      if (request.method === 'DELETE' && path === '/api/v1/hf/orders/cancelAll') return ok({});
      throw new Error(`Unexpected KuCoin Spot request ${request.method} ${request.url}`);
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    'kucoinspot',
    {
      key: 'replay-key',
      secret: 'replay-secret',
      passphrase: 'replay-passphrase',
      transport: directTransport,
      ...(secondary ? { secondary_key: 'read-key', secondary_secret: 'read-secret', secondary_passphrase: 'read-passphrase' } : {}),
      extra: { kucoinBroker: { partner: 'partner-test', brokerKey: 'broker-key', brokerName: 'Broker Test' } },
    },
    store,
  );
  exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));
  exchange.on('error', (error: unknown) => errors.push(error));
  const bounded = <T>(operation: Promise<T>) =>
    new Promise<T>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(JSON.stringify({ errors, loaded: store.loaded, requests: requests.map((r) => r.url) }))),
        3000,
      );
      operation.then(
        (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        (error) => {
          clearTimeout(timer);
          reject(error);
        },
      );
    });
  try {
    await bounded(exchange.start());
    expect(store.markets[0]).toMatchObject({
      id: 'BTC-USDT',
      symbol: 'BTC-USDT',
      isSpot: true,
      contractSize: 1,
      precision: { amount: 0.001, price: 0.1 },
    });
    expect(store.orders[0]).toMatchObject({ id: 'seed-order', amount: 2, filled: 0.5, remaining: 1.5 });
    expect(store.balance).toMatchObject({ total: 1000, used: 100, free: 900, upnl: 0 });
    expect(store.fetchedLeverage['BTC-USDT']).toMatchObject({
      buyLeverage: 1,
      sellLeverage: 1,
      marginType: MarginType.Isolated,
    });
    expect(store.positions).toEqual([]);
    expect(sockets).toHaveLength(2);
    expect(sockets.every((url) => url.startsWith('wss://spot.example.test/endpoint?token='))).toBe(true);
    store.update({ orders: [] });
    const beforeLookup = requests.length;
    expect(await bounded(exchange.findOrderByClientOrderId('BTC-USDT', 'seed-caller'))).toMatchObject({
      found: true,
      order: { id: 'seed-order', clientOrderId: 'seed-caller' },
    });
    expect(requests).toHaveLength(beforeLookup);
    expect(await bounded(exchange.findOrderByClientOrderId('BTC-USDT', 'missing'))).toEqual({
      found: false,
      definitive: false,
    });
    const lookup = requests.at(-1)!;
    expect(lookup.url).toBe(
      'https://api.kucoin.com/api/v1/hf/orders/active/page?symbol=BTC-USDT&pageNum=1&pageSize=50',
    );
    const uncached = { id: 'uncached-id', symbol: 'BTC-USDT', type: OrderType.Limit } as Order;
    expect(await bounded(exchange.updateOrder({ order: uncached, update: { price: 95.26, amount: 1.2345 } }))).toEqual([
      'uncached-id',
    ]);
    const amend = requests.at(-1)!;
    expect(amend.url).toBe('https://api.kucoin.com/api/v1/hf/orders/alter');
    expect(JSON.parse(amend.body!)).toEqual({
      symbol: 'BTC-USDT',
      orderId: 'uncached-id',
      newPrice: '95.3',
      newSize: '1.235',
    });
    await bounded(exchange.cancelOrders([uncached]));
    expect(requests.at(-1)!.url).toBe('https://api.kucoin.com/api/v1/hf/orders/uncached-id?symbol=BTC-USDT');
    await bounded(exchange.cancelAllOrders());
    expect(requests.at(-1)!.url).toBe('https://api.kucoin.com/api/v1/hf/orders/cancelAll');
    expect(
      await bounded(
        exchange.placeOrder({
          symbol: 'BTC-USDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 100.26,
          amount: 1.2345,
          clientOrderId: 'Caller_01-id',
        }),
      ),
    ).toEqual(['placed-id']);
    expect(JSON.parse(requests.at(-1)!.body!)).toEqual({
      symbol: 'BTC-USDT',
      clientOid: 'Caller_01-id',
      type: 'limit',
      side: 'buy',
      price: '100.3',
      size: '1.235',
      timeInForce: 'GTC',
    });
    expect(await bounded(exchange.getAccount())).toEqual({
      userId: 'main-id',
      subId: 'sub-id',
      affiliateId: 'broker-id',
    });
    expect(await bounded(exchange.fetchHistoricalFills({ symbol: 'BTC-USDT', limit: 10 }))).toEqual([
      {
        timestamp: 1700000001000,
        symbol: 'BTC-USDT',
        orderId: 'seed-order',
        price: 89.5,
        qty: 0.2,
        side: OrderSide.Buy,
        commission: 0.01,
        commissionAsset: 'USDT',
      },
    ]);
    for (const request of requests.filter((r) => ['/api/v2/symbols', '/api/v1/market/allTickers'].includes(new URL(r.url).pathname)))
      expect(request.headers['KC-API-KEY']).toBeUndefined();
    const signedRequests = requests.filter((r) => r.headers['KC-API-KEY']);
    expect(signedRequests.length).toBeGreaterThanOrEqual(9);
    for (const request of signedRequests) {
      const timestamp = request.headers['KC-API-TIMESTAMP'];
      const url = new URL(request.url);
      const rawPath =
        url.pathname + (url.search ? '?' + [...url.searchParams].map(([k, v]) => `${k}=${v}`).join('&') : '');
      const read = secondary && request.method === 'GET';
      const key = read ? 'read-key' : 'replay-key';
      const secret = read ? 'read-secret' : 'replay-secret';
      const passphrase = read ? 'read-passphrase' : 'replay-passphrase';
      expect(request.headers['KC-API-KEY']).toBe(key);
      expect(request.headers['KC-API-KEY-VERSION']).toBe('2');
      expect(request.headers['KC-API-SIGN']).toBe(
        hmac(secret, timestamp + request.method + rawPath + (request.body ?? '')),
      );
      expect(request.headers['KC-API-PASSPHRASE']).toBe(hmac(secret, passphrase));
      expect(request.headers['KC-API-PARTNER']).toBe('partner-test');
      expect(request.headers['KC-API-PARTNER-SIGN']).toBe(hmac('broker-key', timestamp + 'partner-test' + key));
      expect(request.headers['KC-BROKER-NAME']).toBe('Broker Test');
      expect(request.headers['KC-API-PARTNER-VERIFY']).toBe('true');
    }
    expect(privateSocket).toBeDefined();
    const event = JSON.stringify({
      type: 'message',
      topic: '/spotMarket/tradeOrdersV2',
      data: {
        orderId: 'seed-order',
        symbol: 'BTC-USDT',
        clientOid: 'seed-caller',
        type: 'match',
        orderType: 'limit',
        status: 'match',
        side: 'buy',
        price: '90',
        size: '2',
        filledSize: '0.7',
        remainSize: '1.3',
        matchSize: '0.2',
        matchPrice: '89.5',
        tradeId: 'execution-1',
        ts: 1700000000001000000,
      },
    });
    privateSocket!.onMessage(event);
    privateSocket!.onMessage(event);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(fills.map(({ id, amount, price }) => ({ id, amount, price }))).toEqual([
      { id: 'seed-order', amount: 0.2, price: 89.5 },
    ]);
    expect(store.orders.find((order) => order.id === 'seed-order')).toMatchObject({ filled: 0.7, remaining: 1.3 });
    expect(errors).toEqual([]);
  } finally {
    exchange.dispose();
  }
}, 10000);
