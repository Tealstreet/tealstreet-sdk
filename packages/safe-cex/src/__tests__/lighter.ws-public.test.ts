import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import '../transport/registerTealstreetTransport';

import { ENDPOINTS, resolveLighterDeploymentProfile } from '../exchanges/lighter/lighter.types';
import { LighterPublicWebsocket } from '../exchanges/lighter/lighter.ws-public';
import { resolveExchangeRouting } from '../transport/tealstreet/routing/resolveExchangeRouting';

// Lighter's public proxy WS host, now sourced from the routing table (was WS_PROXY_URL.public).
const WS_PROXY_PUBLIC = resolveExchangeRouting('lighter', { tealstreetWsRoute: true }).wsPublic.host!;
const WS_PROXY_PUBLIC_ROBINHOOD = resolveExchangeRouting('lighter-robinhood', {
  tealstreetWsRoute: true,
}).wsPublic.host!;
const LIGHTER_MAINNET_WS = resolveLighterDeploymentProfile({ exchangeName: 'lighter', testnet: false }).wsUrl;
const LIGHTER_ROBINHOOD_WS = resolveLighterDeploymentProfile({
  exchangeName: 'lighter-robinhood',
  testnet: false,
}).wsUrl;

const OPEN_STATE = typeof WebSocket !== 'undefined' ? WebSocket.OPEN : 1;

const lighterMarketStore = () => ({
  markets: [{ id: '1', symbol: 'BTCUSDT' }],
  marketsBySymbol: {
    BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
  },
  positions: [],
  tickers: [],
});

const createMockParent = (overrides?: Partial<Record<string, unknown>>) =>
  ({
    name: 'lighter',
    lighterDeploymentProfile: resolveLighterDeploymentProfile({ exchangeName: 'lighter', testnet: false }),
    options: {
      tealstreetWsRoute: true,
      testnet: false,
    },
    isDisposed: false,
    log: vi.fn(),
    emitter: {
      emit: vi.fn(),
      on: vi.fn(),
      off: vi.fn(),
    },
    syncSocketHealthToStore: vi.fn(),
    store: {
      markets: [],
      marketsBySymbol: {},
      positions: [],
      tickers: [],
    },
    ...overrides,
  }) as any;

const createMockWs = () =>
  ({
    readyState: OPEN_STATE,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
    send: vi.fn(),
  }) as any;

describe('LighterPublicWebsocket routing and backoff', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('uses proxy public endpoint immediately when ws route is enabled', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();

    const firstUrl = String(createWebsocketSpy.mock.calls[0]?.[0] || '');
    expect(firstUrl).toContain(WS_PROXY_PUBLIC);
    expect(firstUrl).not.toContain(LIGHTER_MAINNET_WS);
  });

  it('uses direct public endpoint when ws route is disabled', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: false,
        testnet: false,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();

    const firstUrl = String(createWebsocketSpy.mock.calls[0]?.[0] || '');
    expect(firstUrl).toContain(LIGHTER_MAINNET_WS);
    expect(firstUrl).not.toContain(WS_PROXY_PUBLIC);
  });

  it('uses the Robinhood public proxy endpoint when ws route is enabled', () => {
    const parent = createMockParent({
      name: 'lighter-robinhood',
      lighterDeploymentProfile: resolveLighterDeploymentProfile({
        exchangeName: 'lighter-robinhood',
        testnet: false,
      }),
    });
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();

    const firstUrl = String(createWebsocketSpy.mock.calls[0]?.[0] || '');
    expect(firstUrl).toContain(WS_PROXY_PUBLIC_ROBINHOOD);
    expect(firstUrl).not.toContain(WS_PROXY_PUBLIC);
    expect(firstUrl).not.toContain(LIGHTER_ROBINHOOD_WS);
  });

  it('primes ticker REST fallback immediately on socket open', async () => {
    const getMock = vi.fn().mockResolvedValue({
      market_stats: {
        '1': {
          market_id: 1,
          symbol: 'BTCUSDT',
          mark_price: '100',
          index_price: '100',
          last_trade_price: '100',
          best_bid_price: '99',
          best_ask_price: '101',
        },
      },
    });
    const parent = createMockParent({
      lighterApiClient: {
        get: getMock,
      },
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [
          {
            id: '1',
            symbol: 'BTCUSDT',
            bid: 0,
            ask: 0,
            last: 0,
            mark: 0,
            index: 0,
            percentage: 0,
            openInterest: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 0,
            quoteVolume: 0,
          },
        ],
        updateTicker: vi.fn(),
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();

    ws.onOpen();
    await Promise.resolve();

    expect(getMock).toHaveBeenCalledWith(ENDPOINTS.EXCHANGE_STATS, expect.any(Object));
  });

  it('primes ticker REST fallback while proxy socket is still connecting', async () => {
    const getMock = vi.fn().mockResolvedValue({
      market_stats: {
        '1': {
          market_id: 1,
          symbol: 'BTCUSDT',
          mark_price: '100',
          index_price: '100',
          last_trade_price: '100',
        },
      },
    });
    const parent = createMockParent({
      lighterApiClient: {
        get: getMock,
      },
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [
          {
            id: '1',
            symbol: 'BTCUSDT',
            bid: 0,
            ask: 0,
            last: 0,
            mark: 0,
            index: 0,
            percentage: 0,
            openInterest: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 0,
            quoteVolume: 0,
          },
        ],
        updateTicker: vi.fn(),
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();
    await Promise.resolve();

    expect(getMock).toHaveBeenCalledWith(ENDPOINTS.EXCHANGE_STATS, expect.any(Object));
  });

  it('does not double-prime ticker REST fallback across proxy connect and open', async () => {
    const getMock = vi.fn().mockResolvedValue({
      market_stats: {},
    });
    const parent = createMockParent({
      lighterApiClient: {
        get: getMock,
      },
      store: {
        markets: [],
        marketsBySymbol: {},
        positions: [],
        tickers: [],
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();
    ws.onOpen();
    await Promise.resolve();
    await Promise.resolve();

    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(1);
  });

  it('keeps ticker REST fallback active when only non-ticker websocket frames arrive', async () => {
    const getMock = vi.fn().mockResolvedValue({
      market_stats: {},
    });
    const parent = createMockParent({
      lighterApiClient: {
        get: getMock,
      },
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [
          {
            id: '1',
            symbol: 'BTCUSDT',
            bid: 0,
            ask: 0,
            last: 0,
            mark: 0,
            index: 0,
            percentage: 0,
            openInterest: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 0,
            quoteVolume: 0,
          },
        ],
        updateTicker: vi.fn(),
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();
    await Promise.resolve();
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(1);

    ws.onMessage({
      data: JSON.stringify({
        channel: 'order_book:1',
        data: { bids: [], asks: [] },
      }),
    } as MessageEvent);
    await Promise.resolve();

    await vi.advanceTimersByTimeAsync(3_000);
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(2);
  });

  it('stops ticker REST fallback once a websocket ticker hydrates price fields', async () => {
    const getMock = vi.fn().mockResolvedValue({
      market_stats: {},
    });
    const ticker = {
      id: '1',
      symbol: 'BTCUSDT',
      bid: 0,
      ask: 0,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const updateTicker = vi.fn((target, update) => {
      Object.assign(target, update);
    });
    const parent = createMockParent({
      lighterApiClient: {
        get: getMock,
      },
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    ws.connectAndSubscribe();
    await Promise.resolve();

    ws.onMessage({
      data: JSON.stringify({
        channel: 'ticker/1',
        data: {
          last_price: '100',
          best_bid_price: '99',
          best_ask_price: '101',
        },
      }),
    } as MessageEvent);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(3_000);

    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.objectContaining({ last: 100, bid: 99, ask: 101 }));
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(1);
  });

  it('keeps market_stats daily price change in percent units', () => {
    const updateTicker = vi.fn();
    const ticker = {
      id: '100',
      symbol: 'PROVE',
      bid: 0,
      ask: 0,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const parent = createMockParent({
      store: {
        markets: [{ id: '100', symbol: 'PROVE' }],
        marketsBySymbol: {
          PROVE: { id: '100', symbol: 'PROVE' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);

    ws.handleMarketStatsEvent({
      market_stats: {
        '100': {
          market_id: 100,
          symbol: 'PROVE',
          mark_price: '0.32545',
          index_price: '0.32545',
          last_trade_price: '0.32545',
          daily_price_change: 40.5225,
        },
      },
    });

    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.objectContaining({ percentage: 40.5225 }));
  });

  it('applies market_stats by market id when symbol is omitted', () => {
    const updateTicker = vi.fn();
    const ticker = {
      id: '120',
      symbol: 'LIT',
      bid: 0,
      ask: 0,
      last: 11,
      mark: 12,
      index: 12,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const parent = createMockParent({
      store: {
        markets: [{ id: '120', symbol: 'LIT' }],
        marketsBySymbol: {
          LIT: { id: '120', symbol: 'LIT' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);

    ws.handleMarketStatsEvent({
      market_stats: {
        '120': {
          market_id: 120,
          mark_price: '12.228',
          best_bid_price: '12.22',
          best_ask_price: '12.23',
        },
      },
    });

    expect(updateTicker).toHaveBeenCalledWith(
      ticker,
      expect.objectContaining({ mark: 12.228, bid: 12.22, ask: 12.23 }),
    );
  });

  it('does not zero existing ticker prices when market_stats omits price fields', () => {
    const updateTicker = vi.fn();
    const ticker = {
      id: '120',
      symbol: 'LIT',
      bid: 12.22,
      ask: 12.23,
      last: 12.225,
      mark: 12.228,
      index: 12.229,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const parent = createMockParent({
      store: {
        markets: [{ id: '120', symbol: 'LIT' }],
        marketsBySymbol: {
          LIT: { id: '120', symbol: 'LIT' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);

    ws.handleMarketStatsEvent({
      market_stats: {
        '120': {
          market_id: 120,
          current_funding_rate: '0.0012',
        },
      },
    });

    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.not.objectContaining({ mark: 0, last: 0, index: 0 }));
    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.objectContaining({ fundingRate: 0.0001 }));
  });

  it('keeps ticker priceChange24h in percent units when present', () => {
    const updateTicker = vi.fn();
    const ticker = {
      id: '100',
      symbol: 'PROVE',
      bid: 0,
      ask: 0,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const parent = createMockParent({
      store: {
        markets: [{ id: '100', symbol: 'PROVE' }],
        marketsBySymbol: {
          PROVE: { id: '100', symbol: 'PROVE' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);

    ws.handleTickerEvent({
      channel: 'ticker/100',
      data: {
        symbol: 'PROVE',
        priceChange24h: '40.5225',
      },
    });

    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.objectContaining({ percentage: 40.5225 }));
  });

  it('normalizes market_stats funding from 1h percent to internal 8h decimal', () => {
    const updateTicker = vi.fn();
    const ticker = {
      id: '120',
      symbol: 'LIT',
      bid: 0,
      ask: 0,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const parent = createMockParent({
      store: {
        markets: [{ id: '120', symbol: 'LIT' }],
        marketsBySymbol: {
          LIT: { id: '120', symbol: 'LIT' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);

    ws.handleMarketStatsEvent({
      market_stats: {
        '120': {
          market_id: 120,
          symbol: 'LIT',
          current_funding_rate: '0.0012',
        },
      },
    });

    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.objectContaining({ fundingRate: 0.0001 }));
  });

  it('normalizes ticker funding from 1h percent to internal 8h decimal', () => {
    const updateTicker = vi.fn();
    const ticker = {
      id: '100',
      symbol: 'PROVE',
      bid: 0,
      ask: 0,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    const parent = createMockParent({
      store: {
        markets: [{ id: '100', symbol: 'PROVE' }],
        marketsBySymbol: {
          PROVE: { id: '100', symbol: 'PROVE' },
        },
        positions: [],
        tickers: [ticker],
        updateTicker,
      },
    });
    const ws = new LighterPublicWebsocket(parent);

    ws.handleTickerEvent({
      channel: 'ticker/100',
      data: {
        symbol: 'PROVE',
        currentFundingRate: '0.0025',
      },
    });

    expect(updateTicker).toHaveBeenCalledWith(ticker, expect.objectContaining({ fundingRate: 0.0002 }));
  });

  it('cools down ticker REST fallback after a 403', async () => {
    const getMock = vi
      .fn()
      .mockRejectedValueOnce({ response: { status: 403 } })
      .mockResolvedValue({ market_stats: {} });
    const parent = createMockParent({
      lighterApiClient: {
        get: getMock,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();

    await (ws as any).fetchAndApplyTickerFallback();
    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(1);

    (ws as any).startTickerRestFallback('retry during cooldown');
    await vi.advanceTimersByTimeAsync(3_000);
    (ws as any).armTickerRestFallbackWatchdog(1, 'watchdog during cooldown');
    await vi.advanceTimersByTimeAsync(1);
    await (ws as any).fetchAndApplyTickerFallback();

    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(60_000);
    await (ws as any).fetchAndApplyTickerFallback();

    expect(getMock.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.EXCHANGE_STATS)).toHaveLength(2);
  });

  it('probes direct public endpoint only after repeated proxy-route failures', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    for (let i = 0; i < 3; i += 1) {
      ws.connectAndSubscribe();
      (ws as any).onClose({ code: 1006, reason: '' });
      (ws as any).reconnecting = false;
    }

    ws.connectAndSubscribe();

    const latestUrl = String(createWebsocketSpy.mock.calls.at(-1)?.[0] || '');
    expect(latestUrl).toContain(LIGHTER_MAINNET_WS);
    expect(latestUrl).not.toContain(WS_PROXY_PUBLIC);
  });

  it('returns to proxy route when direct fallback fails', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    for (let i = 0; i < 3; i += 1) {
      ws.connectAndSubscribe();
      (ws as any).onClose({ code: 1006, reason: '' });
      (ws as any).reconnecting = false;
    }

    ws.connectAndSubscribe();
    (ws as any).onClose({ code: 1006, reason: '' });
    (ws as any).reconnecting = false;
    ws.connectAndSubscribe();

    const latestUrl = String(createWebsocketSpy.mock.calls.at(-1)?.[0] || '');
    expect(latestUrl).toContain(WS_PROXY_PUBLIC);
  });

  it('pauses repeated direct fallback probes after direct route fails in proxy mode', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    for (let i = 0; i < 3; i += 1) {
      ws.connectAndSubscribe();
      (ws as any).onClose({ code: 1006, reason: '' });
      (ws as any).reconnecting = false;
    }

    ws.connectAndSubscribe();
    (ws as any).onClose({ code: 1006, reason: '' });
    (ws as any).reconnecting = false;

    for (let i = 0; i < 3; i += 1) {
      ws.connectAndSubscribe();
      (ws as any).onClose({ code: 1006, reason: '' });
      (ws as any).reconnecting = false;
    }

    ws.connectAndSubscribe();

    const latestUrl = String(createWebsocketSpy.mock.calls.at(-1)?.[0] || '');
    expect(latestUrl).toContain(WS_PROXY_PUBLIC);
    expect(latestUrl).not.toContain(LIGHTER_MAINNET_WS);
  });

  it('promotes direct route to readonly mode after repeated failures', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: false,
        testnet: false,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createMockWs());

    for (let i = 0; i < 2; i += 1) {
      ws.connectAndSubscribe();
      (ws as any).onClose({ code: 1006, reason: '' });
      (ws as any).reconnecting = false;
    }

    ws.connectAndSubscribe();

    const latestUrl = String(createWebsocketSpy.mock.calls.at(-1)?.[0] || '');
    expect(latestUrl).toContain(LIGHTER_MAINNET_WS);
    expect(latestUrl).toContain('readonly=true');
  });

  it('applies a reconnect floor for rate-limit closes after open', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    (ws as any).openedSinceLastConnect = true;
    (ws as any).hasReceivedMessageSinceConnect = true;
    (ws as any).connectStartedAt = Date.now() - 2_000;
    const setTimeoutSpy = vi.spyOn(global, 'setTimeout');

    (ws as any).onClose({ code: 1013, reason: 'Try again later' });

    const delays = setTimeoutSpy.mock.calls.map((call) => Number(call[1])).filter((delay) => Number.isFinite(delay));
    const reconnectDelay = delays[0];
    expect(reconnectDelay).toBeGreaterThanOrEqual(30_000);
  });

  it('backs off more aggressively on repeated abnormal post-open closes', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const setTimeoutSpy = vi.spyOn(global, 'setTimeout');

    for (let i = 0; i < 2; i += 1) {
      (ws as any).openedSinceLastConnect = true;
      (ws as any).hasReceivedMessageSinceConnect = false;
      (ws as any).connectStartedAt = Date.now() - 1_000;
      (ws as any).onClose({ code: 1006, reason: '' });
      (ws as any).reconnecting = false;
      ws.ws = createMockWs();
    }

    const delays = setTimeoutSpy.mock.calls.map((call) => Number(call[1])).filter((delay) => Number.isFinite(delay));
    expect(delays[1]).toBeGreaterThan(delays[0]);
  });

  it('uses single-channel payloads for OHLCV subscribe and unsubscribe', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: true,
        testnet: false,
        publicDataOnly: false,
      },
      store: lighterMarketStore(),
    });
    const ws = new LighterPublicWebsocket(parent);
    const mockSocket = createMockWs();
    ws.ws = mockSocket;

    const unsubscribe = ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    unsubscribe();

    const payloads = mockSocket.send.mock.calls.map((call: any[]) => JSON.parse(String(call[0] || '{}')));
    expect(payloads.length).toBe(4);

    const [subscribePrimary, subscribeSecondary, unsubscribePrimary, unsubscribeSecondary] = payloads;

    expect(subscribePrimary).toMatchObject({ type: 'subscribe', channel: 'candle/1/1m' });
    expect(subscribeSecondary).toMatchObject({ type: 'subscribe', channel: 'trade/1' });
    expect(unsubscribePrimary).toMatchObject({ type: 'unsubscribe', channel: 'candle/1/1m' });
    expect(unsubscribeSecondary).toMatchObject({ type: 'unsubscribe', channel: 'trade/1' });

    for (const payload of payloads) {
      expect(payload).not.toHaveProperty('channels');
    }
  });

  it('switches to proxy route when direct OHLCV stream stays silent', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: true,
        testnet: false,
        publicDataOnly: false,
      },
      store: lighterMarketStore(),
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const reconnectSpy = vi.spyOn(ws, 'reconnect').mockResolvedValue(undefined);

    ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    vi.advanceTimersByTime(120_001);

    expect(reconnectSpy).toHaveBeenCalledTimes(1);
  });

  it('does not force proxy fallback once OHLCV data arrives', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: true,
        testnet: false,
        publicDataOnly: false,
      },
      store: lighterMarketStore(),
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const reconnectSpy = vi.spyOn(ws, 'reconnect').mockResolvedValue(undefined);

    ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    const candleHandler = (ws.messageHandlers['candle/1/1m'] || [])[0];
    candleHandler?.({
      data: {
        candle: {
          timestamp: Date.now(),
          open: '1',
          high: '2',
          low: '1',
          close: '2',
          volume: '3',
        },
      },
    });

    vi.advanceTimersByTime(120_001);

    expect(reconnectSpy).not.toHaveBeenCalled();
  });

  it('probes reconnect on stale OHLCV when proxy route is disabled', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: false,
        testnet: false,
        publicDataOnly: false,
      },
      store: lighterMarketStore(),
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const reconnectSpy = vi.spyOn(ws, 'reconnect').mockResolvedValue(undefined);

    ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    vi.advanceTimersByTime(120_001);

    expect(reconnectSpy).toHaveBeenCalledTimes(1);
  });

  it('throttles repeated OHLCV recovery probes', () => {
    const parent = createMockParent({
      options: {
        tealstreetWsRoute: true,
        testnet: false,
        publicDataOnly: false,
      },
      store: lighterMarketStore(),
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const reconnectSpy = vi.spyOn(ws, 'reconnect').mockResolvedValue(undefined);

    ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    ws.maybeRecoverOhlcvStream({ symbol: 'BTCUSDT', interval: '1m' }, 'test probe #1');
    ws.maybeRecoverOhlcvStream({ symbol: 'BTCUSDT', interval: '1m' }, 'test probe #2');
    expect(reconnectSpy).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(120_001);
    ws.maybeRecoverOhlcvStream({ symbol: 'BTCUSDT', interval: '1m' }, 'test probe #3');
    expect(reconnectSpy).toHaveBeenCalledTimes(2);
  });

  it('keeps public socket health green during normal quiet periods', () => {
    const parent = createMockParent();
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    ws.lastReceived = Date.now() - 70_000;

    expect(ws.isHealthy).toBe(true);
  });

  it('dispatches order book updates when server emits slash-delimited channels', () => {
    const parent = createMockParent({
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [],
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    ws.onMessage({
      data: JSON.stringify({
        channel: 'order_book/1',
        type: 'snapshot',
        order_book: {
          bids: [['100', '1']],
          asks: [['101', '2']],
        },
      }),
    } as MessageEvent);

    expect(callback).toHaveBeenCalledTimes(1);
    const latestBook = callback.mock.calls[0]?.[0];
    expect(latestBook?.bids?.length).toBeGreaterThan(0);
    expect(latestBook?.asks?.length).toBeGreaterThan(0);
  });

  it('re-subscribes stale order book stream before forcing reconnect', () => {
    const parent = createMockParent({
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [],
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    const mockSocket = createMockWs();
    ws.ws = mockSocket;
    const reconnectSpy = vi.spyOn(ws, 'reconnect').mockResolvedValue(undefined);

    ws.listenOrderBook('BTCUSDT', vi.fn());

    vi.advanceTimersByTime(90_001);

    expect(reconnectSpy).not.toHaveBeenCalled();
    const payloadsAfterFirstTimeout = mockSocket.send.mock.calls.map((call: any[]) =>
      JSON.parse(String(call[0] || '{}')),
    );
    expect(payloadsAfterFirstTimeout).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ type: 'unsubscribe', channel: 'order_book/1' }),
        expect.objectContaining({ type: 'subscribe', channel: 'order_book/1' }),
      ]),
    );

    vi.advanceTimersByTime(90_001);

    expect(reconnectSpy).toHaveBeenCalledTimes(1);
  });

  it('falls back to REST orderbook when initial WS snapshot never arrives', async () => {
    const getOrderBookOrders = vi.fn().mockResolvedValue({
      bids: [['100', '1']],
      asks: [['101', '2']],
    });
    const parent = createMockParent({
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [],
      },
      orderApi: {
        getOrderBookOrders,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    await vi.advanceTimersByTimeAsync(4_100);

    expect(getOrderBookOrders).toHaveBeenCalledWith(1, 100, expect.objectContaining({ __suppressExchangeError: true }));
    expect(callback).toHaveBeenCalled();
  });

  it('keeps active order books on REST fallback while public socket reconnects', async () => {
    const getOrderBookOrders = vi.fn().mockResolvedValue({
      bids: [['100', '1']],
      asks: [['101', '2']],
    });
    const parent = createMockParent({
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [],
      },
      orderApi: {
        getOrderBookOrders,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    await Promise.resolve();
    await Promise.resolve();
    getOrderBookOrders.mockClear();
    callback.mockClear();

    (ws as any).onClose({ code: 1006, reason: '' });
    await Promise.resolve();
    await Promise.resolve();

    expect(getOrderBookOrders).toHaveBeenCalledWith(1, 100, expect.objectContaining({ __suppressExchangeError: true }));
    expect(callback).toHaveBeenCalledWith(
      expect.objectContaining({
        bids: [expect.objectContaining({ price: 100, amount: 1 })],
        asks: [expect.objectContaining({ price: 101, amount: 2 })],
      }),
    );
  });

  it('primes active order books from REST when public socket opens after reconnect', async () => {
    const getOrderBookOrders = vi.fn().mockResolvedValue({
      bids: [['100', '1']],
      asks: [['101', '2']],
    });
    const parent = createMockParent({
      store: {
        markets: [{ id: '1', symbol: 'BTCUSDT' }],
        marketsBySymbol: {
          BTCUSDT: { id: '1', symbol: 'BTCUSDT' },
        },
        positions: [],
        tickers: [],
      },
      orderApi: {
        getOrderBookOrders,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    ws.ws = createMockWs();
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    await Promise.resolve();
    await Promise.resolve();
    getOrderBookOrders.mockClear();
    callback.mockClear();

    ws.onOpen();
    await Promise.resolve();
    await Promise.resolve();

    expect(getOrderBookOrders).toHaveBeenCalledWith(1, 100, expect.objectContaining({ __suppressExchangeError: true }));
    expect(callback).toHaveBeenCalled();
  });

  it('cools down orderbook REST fallback after a 403', async () => {
    const getOrderBookOrders = vi
      .fn()
      .mockRejectedValueOnce({ response: { status: 403 } })
      .mockResolvedValue({
        bids: [['100', '1']],
        asks: [['101', '2']],
      });
    const parent = createMockParent({
      orderApi: {
        getOrderBookOrders,
      },
    });
    const ws = new LighterPublicWebsocket(parent);
    const callback = vi.fn();
    const orderBook = { bids: [], asks: [] };

    await (ws as any).fetchAndEmitOrderBookFromRest('order_book:50', '50', orderBook, callback);
    expect(getOrderBookOrders).toHaveBeenCalledTimes(1);

    (ws as any).startOrderBookRestFallback('order_book:50', '50', orderBook, callback, 'retry during cooldown');
    await vi.advanceTimersByTimeAsync(4_100);
    await (ws as any).fetchAndEmitOrderBookFromRest('order_book:50', '50', orderBook, callback);
    expect(getOrderBookOrders).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(60_000);
    await (ws as any).fetchAndEmitOrderBookFromRest('order_book:50', '50', orderBook, callback);
    expect(getOrderBookOrders).toHaveBeenCalledTimes(2);
  });
});
