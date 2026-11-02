import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ReconnectingWebSocket } from './ReconnectingWebsocket';

/** A browser WebSocket stand-in that records each construction. */
class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static made: FakeWebSocket[] = [];
  CONNECTING = 0;
  OPEN = 1;
  CLOSING = 2;
  CLOSED = 3;
  readyState = 0;
  binaryType = 'blob';
  constructor(public url: string) {
    FakeWebSocket.made.push(this);
  }
  addEventListener() {}
  removeEventListener() {}
  close() {
    this.readyState = 3;
  }
  send() {}
}

// The fake never opens: a long connect timeout keeps each made socket the only attempt.
const options = {
  WebSocket: FakeWebSocket,
  minReconnectionDelay: 1000,
  maxReconnectionDelay: 10_000,
  connectionTimeout: 600_000,
};

describe('ReconnectingWebSocket URL provider', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, 'log').mockImplementation(() => {});
    FakeWebSocket.made = [];
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('retries a rejected URL provider on its normal backoff instead of stopping', async () => {
    let calls = 0;
    const provider = vi.fn(async () => {
      calls += 1;
      if (calls < 3) throw new Error('no token yet');
      return 'wss://example.test/account';
    });
    const errors: unknown[] = [];
    const socket = new ReconnectingWebSocket(provider, undefined, options);
    socket.addEventListener('error', (event) => errors.push(event));
    await vi.advanceTimersByTimeAsync(30_000);
    expect(provider).toHaveBeenCalledTimes(3);
    expect(FakeWebSocket.made).toHaveLength(1);
    expect(errors).toHaveLength(2);
    socket.close();
  });

  it('a reconnect replaces a never-settling URL provider, and the stale one cannot add a socket', async () => {
    let settleStale: ((url: string) => void) | undefined;
    let calls = 0;
    const provider = vi.fn(() => {
      calls += 1;
      if (calls === 1) {
        return new Promise<string>((resolve) => {
          settleStale = resolve;
        });
      }
      return Promise.resolve('wss://example.test/account?attempt=2');
    });
    const socket = new ReconnectingWebSocket(provider, undefined, options);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeWebSocket.made).toHaveLength(0);
    // What the exchange-server worker's watchdog does after a minute with no attempt.
    socket.reconnect(4002, 'connect loop stalled');
    await vi.advanceTimersByTimeAsync(5_000);
    expect(FakeWebSocket.made.map((made) => made.url)).toEqual(['wss://example.test/account?attempt=2']);
    settleStale?.('wss://example.test/account?attempt=1');
    await vi.advanceTimersByTimeAsync(5_000);
    expect(FakeWebSocket.made).toHaveLength(1);
    socket.close();
  });
});
