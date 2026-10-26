import { describe, expect, it, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { GrvtPrivateWebSocket } from '../reference/grvt/grvt.ws-private';
import { GrvtPublicWebSocket } from '../reference/grvt/grvt.ws-public';
import { BaseWebSocket } from '../../exchanges/base.ws';
import { DefaultStore } from '../../store/store.base';
import { ChaserOrderInstance } from '../../webworker/chaserOrder/ChaserOrderInstance';
import { OrderSide } from '../../types';

const makeExchange = () => {
  const exchange = new GrvtExchange('grvt' as any, {} as any, new DefaultStore());
  exchange.grvtTradingAccountId = '7';
  return exchange;
};

describe('GRVT fill-verification P0 regression', () => {
  it('keeps GRVT runtime sources free of console diagnostics that could expose credentials', () => {
    const grvtDirectory = resolve(process.cwd(), 'src/exchanges/grvt');
    const runtimeSources = readdirSync(grvtDirectory).filter((sourceFile) => /^grvt\..+\.ts$/.test(sourceFile));

    for (const sourceFile of runtimeSources) {
      const source = readFileSync(resolve(grvtDirectory, sourceFile), 'utf8');
      expect(source).not.toMatch(/\bconsole\.(?:debug|error|info|log|trace|warn)\b/);
    }
  });

  it('keeps GRVT signing, private-order, and shared failed-send diagnostics payload-free', () => {
    const grvtDirectory = resolve(process.cwd(), 'src/exchanges/grvt');
    const exchangeSource = readFileSync(resolve(grvtDirectory, 'grvt.exchange.ts'), 'utf8');
    const privateWsSource = readFileSync(resolve(grvtDirectory, 'grvt.ws-private.ts'), 'utf8');
    const publicWsSource = readFileSync(resolve(grvtDirectory, 'grvt.ws-public.ts'), 'utf8');
    const baseWsSource = readFileSync(resolve(process.cwd(), 'src/exchanges/base.ws.ts'), 'utf8');

    expect(exchangeSource).not.toMatch(/JSON\.stringify\(/);
    expect(privateWsSource).not.toMatch(/JSON\.stringify\(\s*(?:feed|json\.(?:error|result))/);
    expect(privateWsSource).not.toMatch(/feed\.s=\$\{|feed\.s\)/);
    expect(publicWsSource).not.toMatch(/JSON\.stringify\(json\.error\)/);
    expect(baseWsSource).not.toMatch(/Message preview:|message\.substring\(0, 100\)/);
  });

  it('logs safe metadata, never an auth frame or malformed order signature', () => {
    const authFrameSecret = 'session-cookie-secret-never-log';
    const signatureSecret = 'eip712-signature-never-log';
    const baseParent = {
      isDisposed: false,
      store: {},
      emitter: {},
      log: vi.fn(),
    };
    const baseWs = new BaseWebSocket(baseParent as any);
    baseWs.ws = { readyState: WebSocket.CLOSED } as any;
    baseWs.reconnect = vi.fn();
    const authFrame = JSON.stringify({
      type: 'auth',
      session_cookie: authFrameSecret,
      authorization: 'Bearer api-secret-never-log',
    });

    baseWs.send(authFrame);

    const baseLog = baseParent.log.mock.calls.at(-1)?.[0] as string;
    expect(baseLog).toContain(`messageLength: ${authFrame.length}`);
    expect(baseLog).toContain('messageType: string');
    expect(baseLog).not.toContain(authFrameSecret);
    expect(baseLog).not.toContain('api-secret-never-log');

    const privateParent = {
      isDisposed: false,
      options: {},
      store: {
        marketsById: { BTC_USDT_Perp: { id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' } },
        markets: [{ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' }],
      },
      emitter: { emit: vi.fn() },
      log: vi.fn(),
      emitAndLogError: vi.fn(),
    };
    const privateWs = new GrvtPrivateWebSocket(privateParent as any);

    expect(
      (privateWs as any).parseOrder({
        l: [{ i: 'BTC_USDT_Perp' }],
        s: { r: signatureSecret, s1: signatureSecret, v: 27 },
      }),
    ).toBeNull();

    const privateLogs = privateParent.log.mock.calls.map(([message]) => message).join('\n');
    expect(privateLogs).toContain('parseOrder: missing state (event=order');
    expect(privateLogs).not.toContain(signatureSecret);
  });

  it('replays one public ticker topic per market across reconnects and disposes via BaseWebSocket', async () => {
    vi.useFakeTimers();
    const send = vi.fn();
    const terminate = vi.fn();
    const close = vi.fn();
    const parent = {
      isDisposed: false,
      options: {},
      store: {
        markets: [
          { id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' },
          { id: 'ETH_USDT_Perp', symbol: 'ETH_USDT_PERP' },
        ],
      },
      emitter: { on: vi.fn(), off: vi.fn() },
      emitAndLogError: vi.fn(),
      log: vi.fn(),
    };
    const websocket = new GrvtPublicWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN, terminate, close } as any;
    websocket.send = send;
    websocket.trackLastReceived = vi.fn();

    websocket.onOpen();
    await vi.runAllTimersAsync();
    websocket.onOpen();
    await vi.runAllTimersAsync();

    const tickerSubscriptions = send.mock.calls
      .map(([message]) => JSON.parse(message))
      .filter((message) => message.method === 'subscribe' && message.params.stream === 'v1.ticker.d');
    expect(tickerSubscriptions).toHaveLength(4); // two topics replayed on each of two sockets
    expect(new Set(tickerSubscriptions.map((message) => message.id))).toHaveLength(2);
    expect((websocket as any).subscriptions.size).toBe(2);

    websocket.dispose();
    expect((websocket as any).subscriptions.size).toBe(0);
    expect((websocket as any).tickerSubscriptions.size).toBe(0);
    expect(terminate).toHaveBeenCalledOnce();
    expect(close).toHaveBeenCalledOnce();
    expect(websocket.ws).toBeUndefined();
    vi.useRealTimers();
  });

  it('removes the logical ticker topic when the public consumer unsubscribes', () => {
    const send = vi.fn();
    const parent = {
      isDisposed: false,
      options: {},
      store: { markets: [] },
      emitter: { on: vi.fn(), off: vi.fn() },
      emitAndLogError: vi.fn(),
      log: vi.fn(),
    };
    const websocket = new GrvtPublicWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN } as any;
    websocket.send = send;
    const subscriptionId = websocket.subscribeToTicker('BTC_USDT_Perp', 500, 'v1.ticker.d');
    (websocket as any).tickerSubscriptions.set('BTC_USDT_Perp', subscriptionId);

    websocket.unsubscribeFromTicker(subscriptionId);

    expect((websocket as any).subscriptions.has(subscriptionId)).toBe(false);
    expect((websocket as any).tickerSubscriptions.has('BTC_USDT_Perp')).toBe(false);
    expect(JSON.parse(send.mock.calls[1][0])).toMatchObject({ method: 'unsubscribe', params: { subscription_id: subscriptionId } });
  });

  it('cancels public ticker batches before disposal so delayed work cannot recreate subscriptions', async () => {
    vi.useFakeTimers();
    const send = vi.fn();
    const parent = {
      isDisposed: false,
      options: {},
      store: {
        markets: Array.from({ length: 21 }, (_, index) => ({ id: `MARKET_${index}`, symbol: `MARKET_${index}` })),
      },
      emitter: { on: vi.fn(), off: vi.fn() },
      emitAndLogError: vi.fn(),
      log: vi.fn(),
    };
    const websocket = new GrvtPublicWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN, terminate: vi.fn(), close: vi.fn() } as any;
    websocket.send = send;

    websocket.subscribeToAllTickers();
    websocket.dispose();
    await vi.runAllTimersAsync();

    expect(send).not.toHaveBeenCalled();
    expect((websocket as any).subscriptions.size).toBe(0);
    expect((websocket as any).tickerSubscriptions.size).toBe(0);
    expect((websocket as any).tickerBatchTimers.size).toBe(0);
    vi.useRealTimers();
  });

  it('cancels stale ticker batches on reconnect', async () => {
    vi.useFakeTimers();
    const send = vi.fn();
    const parent = {
      isDisposed: false,
      options: {},
      store: {
        markets: Array.from({ length: 21 }, (_, index) => ({ id: `MARKET_${index}`, symbol: `MARKET_${index}` })),
      },
      emitter: { on: vi.fn(), off: vi.fn() },
      emitAndLogError: vi.fn(),
      log: vi.fn(),
    };
    const websocket = new GrvtPublicWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN } as any;
    websocket.send = send;
    websocket.trackLastReceived = vi.fn();

    websocket.onOpen();
    await vi.advanceTimersByTimeAsync(0); // first 20 subscriptions, one stale batch remains
    websocket.onOpen(); // reconnect must invalidate that stale final batch
    await vi.runAllTimersAsync();

    expect((websocket as any).tickerSubscriptions.size).toBe(21);
    expect(send.mock.calls.filter(([message]) => JSON.parse(message).params?.selectors?.[0] === 'MARKET_20@500')).toHaveLength(1);

    vi.useRealTimers();
  });

  it('cancels queued ticker batches when a ticker unsubscribes', async () => {
    vi.useFakeTimers();
    const parent = {
      isDisposed: false,
      options: {},
      store: {
        markets: Array.from({ length: 21 }, (_, index) => ({ id: `MARKET_${index}`, symbol: `MARKET_${index}` })),
      },
      emitter: { on: vi.fn(), off: vi.fn() },
      emitAndLogError: vi.fn(),
      log: vi.fn(),
    };
    const websocket = new GrvtPublicWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN } as any;
    websocket.send = vi.fn();

    websocket.subscribeToAllTickers();
    await vi.advanceTimersByTimeAsync(0); // first 20 subscriptions, final batch remains queued
    websocket.unsubscribeFromTicker((websocket as any).tickerSubscriptions.get('MARKET_0'));
    await vi.runAllTimersAsync();

    expect((websocket as any).tickerSubscriptions.has('MARKET_0')).toBe(false);
    expect((websocket as any).tickerSubscriptions.has('MARKET_20')).toBe(false);
    expect((websocket as any).tickerBatchTimers.size).toBe(0);
    vi.useRealTimers();
  });

  it('fetches leverage only after balance auto-detects the GRVT trading account', async () => {
    const exchange = makeExchange();
    Object.defineProperty(exchange, 'canSign', { value: true });
    exchange.grvtTradingAccountId = undefined;
    const markets = [
      {
        id: 'BTC_USDT_Perp',
        symbol: 'BTC_USDT_PERP',
        limits: { leverage: { min: 1, max: 100 } },
      },
    ];
    const fetchMarkets = vi.fn().mockResolvedValue(markets);
    const updateLimits = vi.fn().mockImplementation(async (currentMarkets) =>
      currentMarkets.map((market: any) => ({
        ...market,
        limits: { ...market.limits, leverage: { ...market.limits.leverage, max: 25 } },
      })),
    );
    const fetchBalance = vi.fn().mockImplementation(async () => {
      expect(updateLimits).not.toHaveBeenCalled();
      exchange.grvtTradingAccountId = '7';
      return { total: 0, free: 0, used: 0, upnl: 0, currencies: {} };
    });

    exchange.apiExtras = { sessionCookie: 'fresh', accountId: 'account' };
    (exchange as any).initExternalPublicData = vi.fn().mockResolvedValue(false);
    exchange.fetchMarkets = fetchMarkets;
    exchange.fetchTickers = vi.fn().mockResolvedValue([]);
    exchange.fetchBalance = fetchBalance;
    (exchange as any).updateMarketLeverageLimits = updateLimits;
    (exchange as any).fetchBuilderAuthorization = vi.fn().mockResolvedValue(undefined);
    exchange.fetchPositions = vi.fn().mockResolvedValue([]);
    exchange.fetchOpenOrders = vi.fn().mockResolvedValue([]);
    exchange.tick = vi.fn();
    exchange.publicWebsocket.connectAndSubscribe = vi.fn();
    exchange.privateWebsocket.connectAndSubscribe = vi.fn();

    await exchange.start();

    expect(fetchBalance).toHaveBeenCalledOnce();
    expect(updateLimits).toHaveBeenCalledWith(markets);
    expect(updateLimits.mock.invocationCallOrder[0]).toBeGreaterThan(fetchBalance.mock.invocationCallOrder[0]);
    expect(exchange.store.markets[0].limits.leverage.max).toBe(25);
  });

  it('does not fetch account leverage after balance fails before account detection', async () => {
    const exchange = makeExchange();
    Object.defineProperty(exchange, 'canSign', { value: true });
    exchange.grvtTradingAccountId = undefined;
    const updateLimits = vi.fn();

    exchange.apiExtras = { sessionCookie: 'fresh', accountId: 'account' };
    (exchange as any).initExternalPublicData = vi.fn().mockResolvedValue(false);
    exchange.fetchMarkets = vi.fn().mockResolvedValue([]);
    exchange.fetchTickers = vi.fn().mockResolvedValue([]);
    exchange.fetchBalance = vi.fn().mockRejectedValue(new Error('balance unavailable'));
    (exchange as any).updateMarketLeverageLimits = updateLimits;
    (exchange as any).fetchBuilderAuthorization = vi.fn().mockResolvedValue(undefined);
    exchange.fetchPositions = vi.fn().mockResolvedValue([]);
    exchange.fetchOpenOrders = vi.fn().mockResolvedValue([]);
    exchange.tick = vi.fn();
    exchange.publicWebsocket.connectAndSubscribe = vi.fn();
    exchange.privateWebsocket.connectAndSubscribe = vi.fn();

    await exchange.start();

    expect(updateLimits).not.toHaveBeenCalled();
  });

  it('refreshes one expired private-WS session and reconnects with the new cookie', async () => {
    const authenticate = vi.fn().mockResolvedValue(true);
    const parent = {
      isDisposed: false,
      options: {},
      apiExtras: { sessionCookie: 'fresh', accountId: 'account' },
      store: { marketsById: {}, markets: [] },
      emitter: { emit: vi.fn() },
      authenticate,
      log: vi.fn(),
      emitAndLogError: vi.fn(),
    };
    const websocket = new GrvtPrivateWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN, close: vi.fn() } as any;
    websocket.trackLastReceived = vi.fn();
    websocket.reconnect = vi.fn().mockResolvedValue(undefined);

    websocket.onMessage({ data: JSON.stringify({ error: { code: 1000, message: 'authenticate first' } }) } as MessageEvent);
    await vi.waitFor(() => expect(authenticate).toHaveBeenCalledOnce());

    expect(websocket.reconnect).toHaveBeenCalledOnce();
    expect(parent.emitAndLogError).not.toHaveBeenCalled();
  });

  it('fails the private socket closed after its bounded session-refresh retry is exhausted', async () => {
    const close = vi.fn();
    const parent = {
      isDisposed: false,
      options: {},
      apiExtras: { sessionCookie: 'fresh', accountId: 'account' },
      store: { marketsById: {}, markets: [] },
      emitter: { emit: vi.fn() },
      authenticate: vi.fn().mockResolvedValue(true),
      log: vi.fn(),
      emitAndLogError: vi.fn(),
    };
    const websocket = new GrvtPrivateWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN, close } as any;
    websocket.trackLastReceived = vi.fn();
    websocket.reconnect = vi.fn().mockResolvedValue(undefined);

    websocket.onMessage({ data: JSON.stringify({ type: 'error', error: 'expired' }) } as MessageEvent);
    await vi.waitFor(() => expect(websocket.reconnect).toHaveBeenCalledOnce());
    websocket.onMessage({ data: JSON.stringify({ type: 'error', error: 'expired again' }) } as MessageEvent);

    expect(parent.emitAndLogError).toHaveBeenCalledWith(expect.stringMatching(/failed closed/i));
    expect(close).toHaveBeenCalledOnce();
  });

  it('keeps the retry budget across auth_success and fails closed on repeated official 1000 errors', async () => {
    const close = vi.fn();
    const terminate = vi.fn();
    const authenticate = vi.fn().mockResolvedValue(true);
    const parent = {
      isDisposed: false,
      options: {},
      apiExtras: { sessionCookie: 'fresh', accountId: 'account' },
      grvtTradingAccountId: '7',
      store: { marketsById: {}, markets: [] },
      emitter: { emit: vi.fn() },
      authenticate,
      log: vi.fn(),
      emitAndLogError: vi.fn(),
    };
    const websocket = new GrvtPrivateWebSocket(parent as any);
    websocket.ws = { readyState: WebSocket.OPEN, close, terminate, send: vi.fn() } as any;
    websocket.trackLastReceived = vi.fn();
    websocket.reconnect = vi.fn().mockResolvedValue(undefined);

    websocket.onMessage({ data: JSON.stringify({ error: { code: 1000 } }) } as MessageEvent);
    await vi.waitFor(() => expect(authenticate).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect((websocket as any).sessionRefreshInFlight).toBe(false));
    websocket.onMessage({ data: JSON.stringify({ type: 'auth_success' }) } as MessageEvent);
    websocket.onMessage({ data: JSON.stringify({ error: { code: 1000 } }) } as MessageEvent);

    expect(authenticate).toHaveBeenCalledOnce();
    expect(parent.emitAndLogError).toHaveBeenCalledWith(
      expect.stringMatching(/failed closed.*GRVT error 1000.*Check the API key/i),
    );
    expect(terminate).toHaveBeenCalledOnce();
    expect(close).toHaveBeenCalledOnce();
    expect(websocket.ws).toBeUndefined();
  });

  it.each([-0.25, 0, Number.NaN, Number.POSITIVE_INFINITY])(
    'never lets a malformed incremental fill enlarge or poison chaser remaining size (%s)',
    (amount) => {
      const instance = new ChaserOrderInstance({} as any, { size: 1, side: OrderSide.Buy } as any);

      instance.addFillAmount('101', 0.25);
      instance.addFillAmount('101', amount);

      expect(instance.remainingSize).toBe(0.75);
    },
  );

  it.each(['-0.25', '0', 'Infinity', 'NaN', '0.25junk', ' 0.25'])
    ('rejects malformed private WS fill size %j before it can reach a chaser', (size) => {
      const instance = new ChaserOrderInstance({} as any, { size: 1, side: OrderSide.Buy } as any);
      const emit = vi.fn((event: string, fill: any) => {
        if (event === 'fill') instance.addFillAmount(fill.orderId, fill.amount);
      });
      const parent = {
        isDisposed: false,
        options: {},
        store: {
          marketsById: {
            BTC_USDT_Perp: { id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' },
          },
          markets: [{ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' }],
        },
        emitter: { emit },
        log: vi.fn(),
        emitAndLogError: vi.fn(),
      };
      const websocket = new GrvtPrivateWebSocket(parent as any);
      websocket.trackLastReceived = vi.fn();

      websocket.onMessage({
        data: JSON.stringify({
          s: 'v1.fill',
          f: { i: 'BTC_USDT_Perp', ib: true, s: size, p: '100', co: '101', ti: `bad-${size}` },
        }),
      } as MessageEvent);

      expect(emit).not.toHaveBeenCalledWith('fill', expect.anything());
      expect(instance.remainingSize).toBe(1);
    });

  it('emits valid private WS fills exactly once with the positive full-string amount', () => {
    const instance = new ChaserOrderInstance({} as any, { size: 1, side: OrderSide.Buy } as any);
    const emit = vi.fn((event: string, fill: any) => {
      if (event === 'fill') instance.addFillAmount(fill.orderId, fill.amount);
    });
    const parent = {
      isDisposed: false,
      options: {},
      store: {
        marketsById: { BTC_USDT_Perp: { id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' } },
        markets: [{ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' }],
      },
      emitter: { emit },
      log: vi.fn(),
      emitAndLogError: vi.fn(),
    };
    const websocket = new GrvtPrivateWebSocket(parent as any);
    websocket.trackLastReceived = vi.fn();

    websocket.onMessage({
      data: JSON.stringify({ s: 'v1.fill', f: { i: 'BTC_USDT_Perp', ib: true, s: '0.25', p: '100', co: '101', ti: 'ok' } }),
    } as MessageEvent);

    expect(emit).toHaveBeenCalledWith('fill', expect.objectContaining({ orderId: '101', amount: 0.25 }));
    expect(instance.remainingSize).toBe(0.75);
  });

  it('aggregates requested nonce IDs from client IDs and mapped real IDs without cross-order matches', async () => {
    const exchange = makeExchange();
    exchange._nonceToOrderId.set('101', '0xaaa');
    exchange.tradingXhr = {
      post: vi.fn().mockResolvedValue({
        data: {
          r: [
            { co: 101, oi: '0xunrelated', s: '0.2' },
            { metadata: { client_order_id: '102' }, order_id: '0xother', size: '0.3' },
            { oi: '0xaaa', s: '0.4' },
            // A present, different client ID is authoritative: do not credit
            // the real-ID mapping to another chaser order.
            { co: 'other-order', oi: '0xaaa', s: '99' },
            // Conflicting client IDs and incomplete records are unsafe.
            { co: '101', metadata: { client_order_id: '102' }, s: '99' },
            { s: '99' },
            { co: '101', s: 'not-a-number' },
          ],
          n: '',
        },
      }),
    } as any;

    const fills = await exchange.fetchFillsForOrderIds([101 as any, '102']);

    expect(fills.get('101')).toBeCloseTo(0.6);
    expect(fills.get('102')).toBeCloseTo(0.3);
    expect(fills.size).toBe(2);
  });

  it('uses the nonce-to-real-ID mapping populated by open-order mapping when a fill omits client_order_id', async () => {
    const exchange = makeExchange();
    const mappedOrder = (exchange as any).mapOrder({
      order_id: '0xreal-501',
      time_in_force: 'GOOD_TILL_TIME',
      post_only: false,
      reduce_only: false,
      legs: [{ instrument: 'BTC_USDT_Perp', is_buying_asset: true, limit_price: '1', size: '2' }],
      state: { status: 'OPEN', book_size: ['2'], traded_size: ['0'], update_time: '1700000000000000000' },
      metadata: { client_order_id: 501 },
    });
    expect(mappedOrder.id).toBe('501');
    expect(exchange._nonceToOrderId.get('501')).toBe('0xreal-501');

    exchange.tradingXhr = {
      post: vi.fn().mockResolvedValue({ data: { r: [{ oi: '0xreal-501', s: '2' }], n: '' } }),
    } as any;

    await expect(exchange.fetchFillsForOrderIds(['501'])).resolves.toEqual(new Map([['501', 2]]));
  });

  it('uses GRVT maximum page size and credits fills found on an older cursor page', async () => {
    const exchange = makeExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { r: [{ co: 'unrelated', s: '1' }], n: 'older-page' } })
      .mockResolvedValueOnce({ data: { r: [{ co: '101', s: '0.75' }], n: '' } });
    exchange.tradingXhr = { post } as any;

    await expect(exchange.fetchFillsForOrderIds(['101'])).resolves.toEqual(new Map([['101', 0.75]]));
    expect(post).toHaveBeenCalledTimes(2);
    expect(post.mock.calls[0][1]).toMatchObject({ sa: '7', l: 1000 });
    expect(post.mock.calls[0][1].st).not.toEqual(post.mock.calls[0][1].et);
    expect(post.mock.calls[1][1]).toMatchObject({ c: 'older-page', l: 1000 });
  });

  it('credits only strict, finite, positive decimal fill amounts', async () => {
    const exchange = makeExchange();
    exchange.tradingXhr = {
      post: vi.fn().mockResolvedValue({
        data: {
          r: [
            { co: '101', s: '0.25' },
            { co: '101', s: 0.5 },
            { co: '101', s: '0.2junk' },
            { co: '101', s: '0' },
            { co: '101', s: '-0.2' },
            { co: '101', s: 'Infinity' },
            { co: '101', s: ' 0.1' },
          ],
          n: '',
        },
      }),
    } as any;

    await expect(exchange.fetchFillsForOrderIds(['101'])).resolves.toEqual(new Map([['101', 0.75]]));
  });

  it.each([
    { pages: [{ r: [], n: 'repeat' }, { r: [], n: 'repeat' }] },
    { pages: [{ r: [], n: 42 }] },
    { pages: [{ r: [] }] },
    { pages: [{ r: 'not-an-array' }] },
  ])('fails closed instead of returning partial results for malformed pagination', async ({ pages }) => {
    const exchange = makeExchange();
    exchange.tradingXhr = {
      post: vi.fn().mockImplementation(async () => ({ data: pages.shift() })),
    } as any;

    await expect(exchange.fetchFillsForOrderIds(['101'])).rejects.toThrow(/fill history/i);
  });

  it('queries from a long-lived order creation time, not a fixed one-hour window', async () => {
    const exchange = makeExchange();
    const createdAtMs = Date.now() - 2 * 60 * 60 * 1000;
    (exchange as any).mapOrder({
      oi: '0xlong-lived',
      m: { co: '101', ct: String(BigInt(createdAtMs) * 1_000_000n) },
      l: [{}],
      s1: {},
    });
    const post = vi.fn().mockResolvedValue({ data: { r: [], n: '' } });
    exchange.tradingXhr = { post } as any;

    await exchange.fetchFillsForOrderIds(['101']);

    expect(post.mock.calls[0][1].st).toBe(String((createdAtMs - 5 * 60 * 1000) * 1_000_000));
  });

  it('uses a safe broad start for an unknown nonce timestamp', async () => {
    const exchange = makeExchange();
    exchange._nonceToOrderId.set('unknown', '0xunknown');
    const post = vi.fn().mockResolvedValue({ data: { r: [], n: '' } });
    exchange.tradingXhr = { post } as any;

    await exchange.fetchFillsForOrderIds(['unknown']);

    expect(post.mock.calls[0][1].st).toBe('0');
  });

  it('keeps creation tracking bounded with the nonce-to-real-ID lifecycle', () => {
    const exchange = makeExchange();
    for (let index = 0; index <= 200; index++) {
      exchange.recordNonce(String(index), `0x${index}`, Date.now() + index);
    }

    expect(exchange._nonceCreationTimes.size).toBe(200);
    expect(exchange._nonceToOrderId.has('0')).toBe(false);
    expect(exchange._nonceCreationTimes.has('0')).toBe(false);
    expect(exchange._nonceToOrderId.get('200')).toBe('0x200');
  });

  it('throws at the bounded page cap instead of omitting older fills', async () => {
    const exchange = makeExchange();
    const post = vi.fn().mockImplementation(async () => ({ data: { r: [], n: `cursor-${post.mock.calls.length}` } }));
    exchange.tradingXhr = { post } as any;

    await expect(exchange.fetchFillsForOrderIds(['unknown'])).rejects.toThrow(/exceeded 20 pages/i);
    expect(post).toHaveBeenCalledTimes(20);
  });
});
