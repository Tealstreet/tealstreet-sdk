import type { BaseExchange } from '../../exchanges/base';

import { BaseWebSocket } from '../../exchanges/base.ws';
import {
  WS_BACKOFF_BASE_DELAY,
  WS_BACKOFF_FACTOR,
  WS_BACKOFF_MAX_DELAY,
  WS_FAILURE_STREAK_PAUSE_MS,
  WS_FAILURE_STREAK_PAUSE_THRESHOLD,
  WS_MIN_RECONNECTION_DELAY,
} from '../../config/config';

const createParent = () =>
  ({
    isDisposed: false,
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    name: 'reconnect-policy-test',
    options: { key: '', publicDataOnly: true },
    store: { update: vi.fn() },
  }) as unknown as BaseExchange;

describe('BaseWebSocket reconnect policy', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-15T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test('grows backoff for repeated failures with the same close code', () => {
    const socket = new BaseWebSocket(createParent());
    const getReconnectDelay = (socket as any).getReconnectDelay as (code?: number) => number;

    expect(getReconnectDelay(1006)).toBe(Math.max(WS_MIN_RECONNECTION_DELAY, WS_BACKOFF_BASE_DELAY));
    expect(getReconnectDelay(1006)).toBe(
      Math.max(WS_MIN_RECONNECTION_DELAY, WS_BACKOFF_BASE_DELAY * WS_BACKOFF_FACTOR),
    );
    expect(getReconnectDelay(1006)).toBe(
      Math.max(WS_MIN_RECONNECTION_DELAY, WS_BACKOFF_BASE_DELAY * WS_BACKOFF_FACTOR ** 2),
    );
  });

  test('resets backoff when the close code changes or the failure window goes stale', () => {
    const socket = new BaseWebSocket(createParent());
    const getReconnectDelay = (socket as any).getReconnectDelay as (code?: number) => number;
    const initialDelay = Math.max(WS_MIN_RECONNECTION_DELAY, WS_BACKOFF_BASE_DELAY);

    getReconnectDelay(1006);
    getReconnectDelay(1006);
    expect(getReconnectDelay(1013)).toBe(initialDelay);

    getReconnectDelay(1013);
    vi.advanceTimersByTime(WS_BACKOFF_MAX_DELAY * 2 + 1);
    expect(getReconnectDelay(1013)).toBe(initialDelay);
  });

  test('activates the reconnect pause at the consecutive-failure threshold', () => {
    const socket = new BaseWebSocket(createParent());
    const registerCloseResult = (socket as any).registerCloseResult as (code?: number) => void;

    for (let i = 0; i < WS_FAILURE_STREAK_PAUSE_THRESHOLD - 1; i++) {
      registerCloseResult(1006);
    }
    expect(socket.isReconnectPauseActive).toBe(false);

    registerCloseResult(1006);

    expect(socket.isReconnectPauseActive).toBe(true);
    expect(socket.reconnectPauseRemainingMs).toBe(WS_FAILURE_STREAK_PAUSE_MS);
  });
});
