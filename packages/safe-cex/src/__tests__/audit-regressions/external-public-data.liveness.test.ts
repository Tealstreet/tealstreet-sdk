import type { BaseExchange } from '../../exchanges/base';

import {
  EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS,
  EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS,
  ExternalPublicDataProvider,
} from '../../exchanges/external-public-data';

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
});
