import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

import { createHmac } from 'node:crypto';

import { expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { OrderSide, OrderType } from '../../types';

const setup = (spot: boolean, cancelReply?: () => Promise<unknown>, options: Partial<ExchangeOptions> = {}, batchReply?: unknown) => {
  let privateMessage: ((text: string) => void) | undefined;
  const symbol = spot ? 'BTCUSDT' : 'BTCUSD_PERP';
  const requests: Parameters<Transport['request']>[0][] = [];
  const subscriptions: Record<string, unknown>[] = [];
  const errors: unknown[] = [];
  const raw = (value: unknown) => ({ status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify(value)) });
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => 1_700_000_000_000,
    connect: (url, handlers) => {
      if (url.includes('/ws-api/')) privateMessage = handlers.onMessage;
      queueMicrotask(handlers.onOpen);
      return {
        close() {},
        send: (text) => {
          const message = JSON.parse(text);
          if (!url.includes('/ws-api/')) return;
          subscriptions.push(message);
          if (message.method === 'userDataStream.subscribe.signature') {
            const p = message.params;
            expect(p.recvWindow).toBe(5000);
            const payload = `apiKey=${p.apiKey}&recvWindow=${p.recvWindow}&timestamp=${p.timestamp}`;
            expect(p.signature).toBe(createHmac('sha256', 'secret').update(payload).digest('hex'));
            queueMicrotask(() =>
              handlers.onMessage(JSON.stringify({ id: message.id, status: 200, result: { subscriptionId: 7 } })),
            );
          } else {
            expect(message.method).toBe('session.subscriptions');
            queueMicrotask(() =>
              handlers.onMessage(JSON.stringify({ id: message.id, status: 200, result: [{ subscriptionId: 7 }] })),
            );
          }
        },
      };
    },
    request: async (request) => {
      requests.push(request);
      const path = new URL(request.url).pathname;
      expect(path.startsWith(spot ? '/api/v3/' : '/dapi/')).toBe(true);
      if (path.endsWith('/exchangeInfo'))
        return raw({
          symbols: [
            {
              symbol,
              ...(spot ? { status: 'TRADING' } : { contractStatus: 'TRADING' }),
              baseAsset: 'BTC',
              quoteAsset: spot ? 'USDT' : 'USD',
              marginAsset: 'BTC',
              contractType: 'PERPETUAL',
              contractSize: 100,
              baseAssetPrecision: 8,
              quoteAssetPrecision: 8,
              filters: [
                { filterType: 'PRICE_FILTER', tickSize: '0.1' },
                { filterType: 'LOT_SIZE', stepSize: spot ? '0.001' : '1', minQty: '1', maxQty: '100' },
                { filterType: 'MARKET_LOT_SIZE', stepSize: '1', minQty: '1', maxQty: '100' },
              ],
            },
          ],
        });
      if (path.endsWith('/24hr'))
        return raw([
          {
            symbol,
            lastPrice: '10000',
            bidPrice: '9999',
            askPrice: '10001',
            priceChangePercent: '1',
            volume: '2',
            quoteVolume: '20000',
            baseVolume: '0.02',
          },
        ]);
      if (path.endsWith('/bookTicker')) return raw([{ symbol, bidPrice: '9999', askPrice: '10001' }]);
      if (path.endsWith('/premiumIndex'))
        return raw([{ symbol, markPrice: '10000', indexPrice: '10000', lastFundingRate: '0', nextFundingTime: 1 }]);
      if (path.endsWith('/account'))
        return raw(
          spot
            ? {
                balances: [
                  { asset: 'USDT', free: '100', locked: '20' },
                  { asset: 'BTC', free: '1', locked: '0' },
                ],
              }
            : {
                assets: [
                  {
                    asset: 'BTC',
                    walletBalance: '1',
                    availableBalance: '0.8',
                    initialMargin: '0.2',
                    unrealizedProfit: '0.01',
                  },
                ],
                positions: [],
              },
        );
      if (path.endsWith('/leverageBracket')) return raw([{ symbol, brackets: [{ initialLeverage: 100 }] }]);
      if (path.endsWith('/listenKey')) return raw({ listenKey: 'key' });
      if (path.endsWith('/dual')) return raw({ dualSidePosition: false });
      if (path.endsWith('/positionRisk'))
        return raw([
          {
            symbol,
            positionSide: 'BOTH',
            positionAmt: '7',
            entryPrice: '9500',
            markPrice: '10000',
            unRealizedProfit: '0.01',
            leverage: '20',
            marginType: 'cross',
            isolatedMargin: '0',
            liquidationPrice: '8000',
          },
        ]);
      if (path.endsWith('/openOrders'))
        return raw(
          spot
            ? [
                {
                  symbol,
                  orderId: 455,
                  clientOrderId: 'MakerOld',
                  type: 'LIMIT_MAKER',
                  side: 'BUY',
                  price: '9999',
                  origQty: '1',
                  executedQty: '0',
                  status: 'NEW',
                  timeInForce: 'GTC',
                },
              ]
            : [
                {
                  symbol,
                  orderId: 456,
                  clientOrderId: 'resting',
                  type: 'LIMIT',
                  side: 'BUY',
                  price: '9000',
                  origQty: '7',
                  executedQty: '2',
                  status: 'PARTIALLY_FILLED',
                  positionSide: 'BOTH',
                },
              ],
        );
      if (path.endsWith('/order') && request.method === 'DELETE') {
        const reply = cancelReply
          ? await cancelReply()
          : {
              symbol,
              orderId: 455,
              origClientOrderId: 'MakerOld',
              clientOrderId: 'CancelMaker',
              origQty: '1',
              executedQty: '0',
              status: 'CANCELED',
            };
        return raw(reply);
      }
      if (path.endsWith('/batchOrders') && batchReply !== undefined) return raw(batchReply);
      if (path.endsWith('/order')) return raw({ orderId: 123 });
      throw new Error(`Unexpected Binance request ${request.method} ${path}`);
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    spot ? 'binancespot' : 'binanceinverse',
    { key: 'key', secret: 'secret', ...options },
    store,
  );
  exchange.emitter.on('error', (e: unknown) => errors.push(e));
  return {
    exchange,
    store,
    requests,
    subscriptions,
    errors,
    symbol,
    privateEvent: (event: unknown) => privateMessage!(JSON.stringify({ subscriptionId: 7, event })),
  };
};

it('loads Spot balances and signed stream, then honors a caller ID through the real WASM boundary', async () => {
  const t = setup(true);
  try {
    await t.exchange.start();
    expect(t.exchange.isSpot).toBe(true);
    expect(t.store.positions).toEqual([]);
    expect(t.store.balance).toMatchObject({ total: 10120, free: 10100, used: 20 });
    const placed = await t.exchange.placeOrderWithClientOrderId({
      symbol: t.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 9999,
      clientOrderId: 'Caller_123',
    });
    expect(placed).toEqual({ orderIds: ['Caller_123'], clientOrderId: 'Caller_123' });
    const request = t.requests.find((r) => r.method === 'POST')!;
    expect(new URL(request.url).searchParams.get('newClientOrderId')).toBe('Caller_123');
    expect(new URL(request.url).searchParams.has('positionSide')).toBe(false);
    expect(t.subscriptions.some((s) => s.method === 'userDataStream.subscribe.signature')).toBe(true);
    const maker = t.store.orders.find((o) => o.id === 'MakerOld')!;
    expect(maker.timeInForce).toBe('PostOnly');
    const replacements = await t.exchange.updateOrder({ order: maker, update: { price: 9998 } });
    expect(replacements).toHaveLength(1);
    expect(replacements[0]).not.toBe('MakerOld');
    const posts = t.requests.filter((r) => r.method === 'POST');
    const replacement = new URL(posts.at(-1)!.url).searchParams;
    expect(replacement.get('type')).toBe('LIMIT_MAKER');
    expect(replacement.has('timeInForce')).toBe(false);
    expect(replacement.get('newClientOrderId')).toBe(replacements[0]);
    expect(
      t.requests.some(
        (r) => r.method === 'DELETE' && new URL(r.url).searchParams.get('origClientOrderId') === 'MakerOld',
      ),
    ).toBe(true);

    expect(t.errors).toEqual([]);
    const before = t.requests.length;
    await expect(t.exchange.fetchPublicTrades(t.symbol, 1)).rejects.toThrow('Spot query is not supported');
    expect(t.requests).toHaveLength(before);
  } finally {
    t.exchange.dispose();
  }
});

it('preserves COIN-M USD contract placement units and coin wallet valuation through WASM', async () => {
  const t = setup(false);
  try {
    await t.exchange.start();
    expect(t.store.markets[0]).toMatchObject({ isInverse: true, contractSize: 100 });
    expect(t.store.balance).toMatchObject({ total: 10000, free: 8000, used: 2000, upnl: 100 });
    expect(t.store.positions[0]).toMatchObject({
      notional: 700,
      contracts: 0.07,
      extra: { contractsNative: 7, unrealizedPnlNative: 0.01 },
    });
    expect(t.store.orders[0]).toMatchObject({
      amount: 7,
      filled: 2,
      remaining: 5,
      extra: { remainingNative: 5, remainingNotional: 500 },
    });
    expect(
      await t.exchange.placeOrder({ symbol: t.symbol, type: OrderType.Market, side: OrderSide.Buy, amount: 3 }),
    ).toEqual(['123']);
    const request = t.requests.find((r) => r.method === 'POST' && r.url.includes('/order?'))!;
    expect(new URL(request.url).searchParams.get('quantity')).toBe('3');
    expect(t.errors).toEqual([]);
    const before = t.requests.length;
    await expect(
      t.exchange.placeOrder({
        symbol: t.symbol,
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 3,
        clientOrderId: 'unsupported',
      }),
    ).rejects.toThrow();
    expect(t.requests).toHaveLength(before);
  } finally {
    t.exchange.dispose();
  }
});

it('rejects a partial COIN-M batch with the native accepted IDs through the SDK', async () => {
  const t = setup(false, undefined, {}, [{ orderId: 321 }, { code: -2010, msg: 'insufficient margin' }]);
  try {
    await t.exchange.start();
    await expect(t.exchange.placeOrders([
      { symbol: t.symbol, type: OrderType.Limit, side: OrderSide.Buy, amount: 2, price: 9000 },
      { symbol: t.symbol, type: OrderType.Limit, side: OrderSide.Buy, amount: 3, price: 9000 },
    ])).rejects.toMatchObject({ message: 'insufficient margin', partialOrderIds: ['321'] });
    expect(t.requests.filter((r) => r.method === 'POST' && r.url.includes('/batchOrders?'))).toHaveLength(1);
    expect(t.requests.filter((r) => r.method === 'POST' && r.url.includes('/order?'))).toHaveLength(0);
    // The SDK command boundary reports the rejection exactly once.
    expect(t.errors).toEqual(['insufficient margin']);
  } finally {
    t.exchange.dispose();
  }
});

it('preserves native Spot currency slices after snapshots and partial updates in worker and main stores', async () => {
  const t = setup(true);
  const mainStore = new DefaultStore();
  const main = new BaseExchange('binancespot', { key: '', secret: '' }, mainStore);
  try {
    await t.exchange.start();
    main.applyWorkerFrame(t.exchange.snapshotFrame()!);
    for (const store of [t.store, mainStore]) {
      expect(store.balance.currencies).toEqual({
        BTC: { free: 1, used: 0, total: 1, upnl: 0 },
        USDT: { free: 100, used: 20, total: 120, upnl: 0 },
      });
    }
    t.privateEvent({ e: 'outboundAccountPosition', B: [{ a: 'BTC', f: '0.25', l: '0.05' }] });
    await vi.waitFor(() =>
      expect(t.store.balance.currencies?.BTC).toEqual({ free: 0.25, used: 0.05, total: 0.3, upnl: 0 }),
    );
    main.applyWorkerFrame(t.exchange.snapshotFrame()!);
    for (const store of [t.store, mainStore]) {
      expect(store.balance.currencies).toEqual({
        BTC: { free: 0.25, used: 0.05, total: 0.3, upnl: 0 },
        USDT: { free: 100, used: 20, total: 120, upnl: 0 },
      });
      expect(store.balance).toMatchObject({ free: 2600, used: 520, total: 3120, upnl: 0 });
      expect(store.balance.coins?.BTC).toEqual({ base: 'BTC', amount: 0.3, notional: 3000 });
    }
    expect(t.errors).toEqual([]);
  } finally {
    main.dispose();
    t.exchange.dispose();
  }
});

it('never replaces a Spot order filled while cancellation is in flight across WASM', async () => {
  let sent!: () => void;
  const cancelSent = new Promise<void>((resolve) => {
    sent = resolve;
  });
  let respond!: (reply: unknown) => void;
  const response = new Promise<unknown>((resolve) => {
    respond = resolve;
  });
  const t = setup(true, () => {
    sent();
    return response;
  });
  try {
    await t.exchange.start();
    const maker = t.store.orders.find((o) => o.id === 'MakerOld')!;
    const pending = t.exchange.updateOrder({ order: maker, update: { price: 9998 } });
    await cancelSent;
    t.privateEvent({
      e: 'executionReport',
      s: t.symbol,
      i: 455,
      c: 'MakerOld',
      S: 'BUY',
      o: 'LIMIT_MAKER',
      f: 'GTC',
      q: '1',
      p: '9999',
      P: '0',
      x: 'TRADE',
      X: 'FILLED',
      z: '1',
      l: '1',
      L: '9998',
      T: 1700000000000,
    });
    respond({
      symbol: t.symbol,
      orderId: 455,
      origClientOrderId: 'MakerOld',
      clientOrderId: 'CancelMaker',
      origQty: '1',
      executedQty: '0.5',
      status: 'CANCELED',
    });
    expect(await pending).toEqual([]);
    expect(t.requests.filter((r) => r.method === 'POST')).toEqual([]);
    await vi.waitFor(() =>
      expect(t.store.orders.find((o) => o.id === 'MakerOld')).toMatchObject({
        status: 'closed',
        filled: 1,
        remaining: 0,
      }),
    );
  } finally {
    t.exchange.dispose();
  }
});

it('rejects an ambiguous Spot cancellation response before replacement across WASM', async () => {
  const t = setup(true, async () => ({}));
  try {
    await t.exchange.start();
    const maker = t.store.orders.find((o) => o.id === 'MakerOld')!;
    await expect(t.exchange.updateOrder({ order: maker, update: { price: 9998 } })).rejects.toThrow();
    expect(t.requests.filter((r) => r.method === 'POST')).toEqual([]);
    expect(t.requests.filter((r) => r.method === 'DELETE')).toHaveLength(1);
  } finally {
    t.exchange.dispose();
  }
});


it.each([true, false])('forwards recvWindow through actual Spot/COIN-M WASM signing (spot=%s)', async (spot) => {
  const t = setup(spot, undefined, { extra: { recvWindow: 4321 } });
  try {
    await t.exchange.start();
    await t.exchange.placeOrder({ symbol: t.symbol, type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 9999 });
    const signed = t.requests.filter((request) => new URL(request.url).searchParams.has('signature'));
    expect(signed.some((request) => request.method === 'GET')).toBe(true);
    expect(signed.some((request) => request.method === 'POST')).toBe(true);
    for (const request of signed) {
      expect(new URL(request.url).searchParams.getAll('recvWindow')).toEqual(['4321']);
      const [query, signature] = new URL(request.url).search.slice(1).split('&signature=');
      expect(signature).toBe(createHmac('sha256', 'secret').update(query).digest('hex'));
      expect(request.headers['X-MBX-APIKEY']).toBe('key');
    }
    if (spot) expect(t.subscriptions.some((message) => message.method === 'userDataStream.subscribe.signature')).toBe(true);
  } finally { t.exchange.dispose(); }
});
