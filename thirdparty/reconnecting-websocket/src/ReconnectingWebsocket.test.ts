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

/** A fake that keeps its listeners, so a test can open and drop it. */
class ControllableWebSocket extends FakeWebSocket {
  static live: ControllableWebSocket[] = [];
  private readonly handlers = new Map<string, (event: unknown) => void>();
  constructor(url: string) {
    super(url);
    ControllableWebSocket.live.push(this);
  }
  addEventListener(type: string, handler: (event: unknown) => void) {
    this.handlers.set(type, handler);
  }
  removeEventListener(type: string) {
    this.handlers.delete(type);
  }
  fire(type: string, event: unknown = {}) {
    this.handlers.get(type)?.(event);
  }
}

describe('ReconnectingWebSocket loop phase', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, 'log').mockImplementation(() => {});
    FakeWebSocket.made = [];
    ControllableWebSocket.live = [];
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it.each([
    { refusal: 503, base: 1000 },
    { refusal: 503, base: 3000 },
    { refusal: 1013, base: 1000 },
    { refusal: 1013, base: 3000 },
  ])(
    'paces router $refusal retries with a $base ms socket jitter base and a ten-second cap',
    async ({ refusal, base }) => {
      const socket = new ReconnectingWebSocket('wss://example.test/account', undefined, {
        ...options,
        WebSocket: ControllableWebSocket,
        minReconnectionDelay: base,
        reconnectionDelayGrowFactor: 1.3,
        reconnectsAtOnce: (event) => event.code === 4100,
      });
      await vi.advanceTimersByTimeAsync(0);
      for (let attempt = 0; attempt < 12; attempt++) {
        const transport = ControllableWebSocket.live[attempt];
        if (refusal === 1013) {
          transport.fire('open');
          transport.fire('close', { code: 1013, reason: 'Try again later' });
        } else transport.fire('error', { message: 'Unexpected server response: 503' });
        expect(socket.retryCount).toBe(attempt + 1);
        const delay = Math.floor(Math.min(base * Math.pow(1.3, attempt), 10_000));
        await vi.advanceTimersByTimeAsync(delay - 1);
        expect(ControllableWebSocket.live).toHaveLength(attempt + 1);
        await vi.advanceTimersByTimeAsync(1);
        expect(ControllableWebSocket.live).toHaveLength(attempt + 2);
      }
      socket.close();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(ControllableWebSocket.live).toHaveLength(13);
    },
  );

  it('drops owner A retry work when login switches to B during backoff', async () => {
    const a = new ReconnectingWebSocket('wss://example.test/owner-A', undefined, {
      ...options,
      WebSocket: ControllableWebSocket,
    });
    await vi.advanceTimersByTimeAsync(0);
    ControllableWebSocket.live[0].fire('close', { code: 1013, reason: 'Try again later' });
    expect(a.loopState.phase).toBe('retry-scheduled');
    a.close();
    const b = new ReconnectingWebSocket('wss://example.test/owner-B', undefined, {
      ...options,
      WebSocket: ControllableWebSocket,
    });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ControllableWebSocket.live.map((socket) => socket.url)).toEqual([
      'wss://example.test/owner-A',
      'wss://example.test/owner-B',
    ]);
    expect(a.loopState.phase).toBe('stopped');
    b.close();
  });

  it('moves through connecting, connected, retry-scheduled and stopped, reporting each change once', async () => {
    const phases: string[] = [];
    const socket = new ReconnectingWebSocket('wss://example.test/account', undefined, {
      ...options,
      WebSocket: ControllableWebSocket,
      onLoopPhase: (phase) => phases.push(phase),
    });
    expect(socket.loopState.phase).toBe('connecting');
    await vi.advanceTimersByTimeAsync(0);
    ControllableWebSocket.live[0].fire('open');
    expect(socket.loopState.phase).toBe('connected');

    // The server drops it: the next attempt waits out its backoff first.
    ControllableWebSocket.live[0].fire('close', { code: 1006, reason: '' });
    expect(socket.loopState).toEqual({ phase: 'retry-scheduled', since: Date.now() });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(socket.loopState.phase).toBe('connecting');

    socket.close();
    expect(socket.loopState.phase).toBe('stopped');
    expect(phases).toEqual(['connecting', 'connected', 'retry-scheduled', 'connecting', 'stopped']);
  });

  it('keeps a nonzero retry count across an open-connection supervisor retry and never duplicates it', async () => {
    const socket = new ReconnectingWebSocket('wss://example.test', undefined, {
      ...options,
      WebSocket: ControllableWebSocket,
    });
    const onClose = vi.fn();
    socket.onclose = onClose;
    await vi.advanceTimersByTimeAsync(0);
    expect(socket.timeoutConnecting()).toBe(true);
    await vi.advanceTimersByTimeAsync(1_000);
    ControllableWebSocket.live[1].fire('open');
    expect(socket.retryCount).toBe(1);
    expect(socket.retryConnection()).toBe(true);
    expect(socket.retryConnection()).toBe(false);
    expect(socket.retryCount).toBe(2);
    expect(onClose.mock.calls.at(-1)?.[0].initiator).toBe('local');
    await vi.advanceTimersByTimeAsync(1_299);
    expect(ControllableWebSocket.live).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(ControllableWebSocket.live).toHaveLength(3);
    socket.terminate();
    expect(socket.retryConnection()).toBe(false);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ControllableWebSocket.live).toHaveLength(3);
  });

  it('a reconnect from stopped resumes the loop', async () => {
    const socket = new ReconnectingWebSocket('wss://example.test/account', undefined, {
      ...options,
      WebSocket: ControllableWebSocket,
    });
    socket.close();
    expect(socket.loopState.phase).toBe('stopped');
    socket.reconnect(4002, 'connect loop repaired');
    expect(socket.loopState.phase).not.toBe('stopped');
  });
});

describe('ReconnectingWebSocket audited connect timeout', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, 'log').mockImplementation(() => {});
    FakeWebSocket.made = [];
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('retains retry count and normal delay when a supervisor times out the attempt', async () => {
    const socket = new ReconnectingWebSocket('wss://example.test', undefined, options);
    const closed = vi.fn();
    socket.onclose = closed;
    await vi.advanceTimersByTimeAsync(0);
    expect(socket.timeoutConnecting()).toBe(true);
    expect(socket.timeoutConnecting()).toBe(false);
    expect(closed).toHaveBeenCalledTimes(1);
    expect(socket.retryCount).toBe(1);
    expect(socket.loopState.phase).toBe('retry-scheduled');
    await vi.advanceTimersByTimeAsync(999);
    expect(FakeWebSocket.made).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeWebSocket.made).toHaveLength(2);
    expect(socket.timeoutConnecting()).toBe(true);
    expect(socket.retryCount).toBe(2);
    await vi.advanceTimersByTimeAsync(1_299);
    expect(FakeWebSocket.made).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeWebSocket.made).toHaveLength(3);
    socket.terminate();
  });

  it('retires a pending URL provider and ignores its late result', async () => {
    let settle: (url: string) => void = () => {};
    const provider = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<string>((resolve) => {
            settle = resolve;
          }),
      )
      .mockReturnValue('wss://example.test/current');
    const socket = new ReconnectingWebSocket(provider, undefined, options);
    const closed = vi.fn();
    socket.onclose = closed;
    await vi.advanceTimersByTimeAsync(0);
    expect(socket.timeoutConnecting()).toBe(true);
    expect(closed).toHaveBeenCalledTimes(1);
    settle('wss://example.test/retired');
    await vi.advanceTimersByTimeAsync(1_000);
    expect(FakeWebSocket.made.map((made) => made.url)).toEqual(['wss://example.test/current']);
    socket.terminate();
  });

  it('does not time out an open or stopped wrapper', async () => {
    const socket = new ReconnectingWebSocket('wss://example.test', undefined, options);
    await vi.advanceTimersByTimeAsync(0);
    FakeWebSocket.made[0].readyState = 1;
    expect(socket.timeoutConnecting()).toBe(false);
    socket.terminate();
    expect(socket.timeoutConnecting()).toBe(false);
  });
});

describe('reconnect diagnostics', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'info').mockImplementation(() => {});
    FakeWebSocket.made = [];
  });
  afterEach(() => vi.useRealTimers());

  it.each([false, true])(
    'logs the resolved route without a function, query, credentials or frame contents (async=%s)',
    async (asyncProvider) => {
      const url = 'wss://fixture-user:fixture-password@example.test/feed?signature=fixture-signature#fixture-fragment';
      const provider = vi.fn(asyncProvider ? async () => url : () => url);
      const socket = new ReconnectingWebSocket(provider, ['fixture-protocol-token'], {
        ...options,
        debug: true,
        startClosed: true,
      });
      try {
        socket.reconnect(1000, 'fixture-reason?signature=fixture-reason-signature');
        await vi.advanceTimersByTimeAsync(1);
        expect(provider).toHaveBeenCalledTimes(1);
        expect(FakeWebSocket.made[0].url).toBe(url);
        FakeWebSocket.made[0].readyState = 1;
        socket.send('{"signature":"fixture-frame-signature"}');
        const logged = [...vi.mocked(console.log).mock.calls, ...vi.mocked(console.info).mock.calls]
          .map((args) => JSON.stringify(args))
          .join('\n');
        expect(logged).toContain('wss://example.test/feed');
        expect(logged).not.toContain(provider.toString());
        expect(logged).not.toMatch(/fixture-|\?signature|=>/);
      } finally {
        socket.close();
      }
    },
  );

  it('uses a redacted placeholder when the provider fails, without logging its error', async () => {
    const socket = new ReconnectingWebSocket(
      async () => {
        throw new Error('fixture-provider-signature');
      },
      [],
      { ...options, startClosed: true },
    );
    try {
      socket.reconnect();
      await vi.advanceTimersByTimeAsync(1);
      const logged = [...vi.mocked(console.log).mock.calls, ...vi.mocked(console.info).mock.calls]
        .flat()
        .map(String)
        .join(' ');
      expect(logged).toContain('[unavailable]');
      expect(logged).not.toMatch(/fixture-provider-signature|=>/);
      expect(FakeWebSocket.made).toHaveLength(0);
    } finally {
      socket.close();
    }
  });
});
