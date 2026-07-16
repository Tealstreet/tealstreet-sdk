import { BaseExchange } from '../../exchanges/base';
import {
  EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS,
  EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS,
  ExternalPublicDataProvider,
} from '../../exchanges/external-public-data';
import { DefaultStore } from '../../store/store.base';

const createParent = () =>
  ({
    emitter: { emit: vi.fn() },
    name: 'gateway-test',
    store: {
      loaded: {},
      markets: [],
      tickers: [],
      update: vi.fn(),
    },
  }) as unknown as BaseExchange;

describe('ExternalPublicDataProvider liveness', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-15T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test('pings a silent connection, then closes it after the stale threshold', () => {
    const provider = new ExternalPublicDataProvider(createParent(), {
      url: 'wss://gateway.example.test',
      exchangeId: 'contract-test',
    });
    const send = vi.fn();
    const close = vi.fn();
    (provider as any).ws = { readyState: WebSocket.OPEN, send, close };

    (provider as any).onOpen();
    send.mockClear();

    vi.advanceTimersByTime(EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS + 1_000);

    expect(send).toHaveBeenCalledTimes(1);
    expect(JSON.parse(send.mock.calls[0][0])).toEqual({
      type: 'ping',
      exchangeId: 'contract-test',
    });

    vi.advanceTimersByTime(
      EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS - EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS,
    );

    expect(close).toHaveBeenCalledTimes(1);
    provider.dispose();
  });

  test('any inbound gateway frame resets the heartbeat', () => {
    const provider = new ExternalPublicDataProvider(createParent(), {
      url: 'wss://gateway.example.test',
      exchangeId: 'contract-test',
    });
    const send = vi.fn();
    const close = vi.fn();
    (provider as any).ws = { readyState: WebSocket.OPEN, send, close };

    (provider as any).onOpen();
    send.mockClear();
    vi.advanceTimersByTime(EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS + 1_000);
    expect(send).toHaveBeenCalledTimes(1);

    (provider as any).onMessage({
      data: JSON.stringify({ type: 'pong', exchangeId: 'contract-test' }),
    });
    send.mockClear();
    vi.advanceTimersByTime(EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS);

    expect(send).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
    provider.dispose();
  });

  test('re-subscribes colon-bearing symbols without truncating them', () => {
    const provider = new ExternalPublicDataProvider(createParent(), {
      url: 'wss://gateway.example.test',
      exchangeId: 'contract-test',
    });
    const send = vi.fn();
    const close = vi.fn();

    provider.subscribeCandle({ symbol: 'HIP3:BTC-USDC', interval: '1m' }, vi.fn());
    (provider as any).ws = { readyState: WebSocket.OPEN, send, close };
    (provider as any).onOpen();

    const candleSubscriptions = send.mock.calls
      .map(([payload]) => JSON.parse(payload))
      .filter((payload) => payload.channel === 'candle');
    expect(candleSubscriptions).toEqual([
      {
        type: 'subscribe',
        exchangeId: 'contract-test',
        channel: 'candle',
        symbol: 'HIP3:BTC-USDC',
        interval: '1m',
      },
    ]);
    provider.dispose();
  });

  test('reports gateway disconnects through the exchange public socket-health bridge', () => {
    const store = new DefaultStore();
    const exchange = new BaseExchange('binance', { key: '', secret: '', publicDataOnly: true }, store);
    const provider = new ExternalPublicDataProvider(exchange, {
      url: 'wss://gateway.example.test',
      exchangeId: 'contract-test',
    });
    const ws: { readyState: number; send: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn> } = {
      readyState: WebSocket.OPEN,
      send: vi.fn(),
      close: vi.fn(),
    };
    (provider as any).ws = ws;
    exchange.externalPublicDataProvider = provider;

    (provider as any).onOpen();
    exchange.syncSocketHealthToStore();
    expect(store.socketHealth).toMatchObject({ publicInitialized: true, publicHealthy: true });

    ws.readyState = WebSocket.CLOSED;
    exchange.syncSocketHealthToStore();
    expect(store.socketHealth).toMatchObject({ publicInitialized: true, publicHealthy: false });

    exchange.dispose();
  });
});
