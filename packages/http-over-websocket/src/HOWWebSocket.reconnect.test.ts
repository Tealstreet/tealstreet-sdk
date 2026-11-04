// Drives the REAL HOWWebSocket + ReconnectingWebSocket + adapter against a
// scripted in-memory tunnel, so connection-lifecycle stalls show up as requests
// that wait out their whole deadline instead of recovering after a reconnect.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createAxiosHttpOverWebsocketAdapter, TradeTunnelTimeoutError } from './axiosHttpOverWebsocketAdapter';
import { configureHOW } from './config';

type ServerMode = {
  /** IP-whitelisted: rpc.auth success right after open, no token needed. */
  autoAuth?: boolean;
  /** rpc.auth frames swallowed on this connection before the server answers one. */
  ignoreAuthFrames?: number;
  /** Opens, then never sends anything (no serverTime, no pong, no replies). */
  silent?: boolean;
  /** rpc.http frames are accepted but not answered (keeps a request in flight). */
  holdHttp?: boolean;
  /** The upgrade never completes (a dial lost on a slow link); the client redials. */
  neverOpen?: boolean;
  /** rpc.http replies arrive this long after the request (a large body on a slow link). */
  replyAfterMs?: number;
};

class FakeTunnelSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  CONNECTING = 0;
  OPEN = 1;
  CLOSING = 2;
  CLOSED = 3;

  static instances: FakeTunnelSocket[] = [];
  static nextModes: ServerMode[] = [];
  static defaultMode: ServerMode = {};

  readyState = 0;
  binaryType = 'blob';
  bufferedAmount = 0;
  extensions = '';
  protocol = '';
  sent: any[] = [];
  mode: ServerMode;
  authFramesSeen = 0;
  private tick?: ReturnType<typeof setInterval>;
  private listeners: Record<string, Array<(event: any) => void>> = { open: [], message: [], close: [], error: [] };

  constructor(readonly url: string) {
    this.mode = FakeTunnelSocket.nextModes.shift() ?? FakeTunnelSocket.defaultMode;
    FakeTunnelSocket.instances.push(this);
    if (!this.mode.neverOpen) setTimeout(() => this.serverAccept(), 10);
  }

  addEventListener(type: string, listener: (event: any) => void) {
    this.listeners[type]?.push(listener);
  }
  removeEventListener(type: string, listener: (event: any) => void) {
    if (this.listeners[type]) this.listeners[type] = this.listeners[type].filter((l) => l !== listener);
  }
  private fire(type: string, event: any) {
    for (const listener of [...(this.listeners[type] ?? [])]) listener(event);
  }

  send(text: string) {
    if (this.readyState !== 1) throw new Error('fake socket not open');
    const frame = JSON.parse(text);
    this.sent.push(frame);
    if (this.mode.silent) return;
    if (frame.type === 'rpc.ping') this.serverSendLater({ type: 'rpc.ping', status: 'success', nonce: frame.nonce });
    if (frame.type === 'rpc.auth') {
      this.authFramesSeen++;
      if (this.authFramesSeen > (this.mode.ignoreAuthFrames ?? 0))
        this.serverSendLater({ type: 'rpc.auth', status: 'success' });
    }
    if (frame.type === 'rpc.http' && !this.mode.holdHttp) {
      const reply = {
        type: 'rpc.http',
        nonce: frame.nonce,
        data: { status: 200, headers: {}, data_raw: '{"retCode":0}' },
      };
      if (this.mode.replyAfterMs) setTimeout(() => this.serverSend(reply), this.mode.replyAfterMs);
      else this.serverSendLater(reply);
    }
  }

  close() {
    if (this.readyState === 3) return;
    this.readyState = 3;
    clearInterval(this.tick);
    setTimeout(() => this.fire('close', { type: 'close', code: 1000, reason: '', wasClean: true }), 0);
  }

  // ---- server side ----
  serverAccept() {
    if (this.readyState !== 0) return;
    this.readyState = 1;
    this.fire('open', { type: 'open' });
    if (this.mode.silent) return;
    if (this.mode.autoAuth) this.serverSendLater({ type: 'rpc.auth', status: 'success' });
    // serverTime arrives on the server's periodic tick, not on open.
    this.tick = setInterval(() => this.serverSend({ type: 'serverTime', serverTime: Date.now() }), 1_000);
  }
  serverSend(json: any) {
    if (this.readyState === 1) this.fire('message', { type: 'message', data: JSON.stringify(json) });
  }
  serverSendLater(json: any) {
    setTimeout(() => this.serverSend(json), 5);
  }
  /** Abnormal drop (network loss / server restart). */
  serverDrop() {
    if (this.readyState === 3) return;
    this.readyState = 3;
    clearInterval(this.tick);
    this.fire('close', { type: 'close', code: 1006, reason: '', wasClean: false });
  }

  static latest() {
    return FakeTunnelSocket.instances[FakeTunnelSocket.instances.length - 1];
  }
}

const instrumentsInfo = (timeout = 20_000): any => ({
  method: 'GET',
  url: 'https://api.bybit.com/v5/market/instruments-info?category=linear',
  baseURL: 'https://api.bybit.com',
  headers: {},
  timeout,
});

type Tracked<T> = { promise: Promise<T>; state: () => 'pending' | 'resolved' | 'rejected'; error: () => any };
const track = <T>(promise: Promise<T>): Tracked<T> => {
  let state: 'pending' | 'resolved' | 'rejected' = 'pending';
  let error: any;
  promise.then(
    () => (state = 'resolved'),
    (e) => {
      state = 'rejected';
      error = e;
    },
  );
  return { promise, state: () => state, error: () => error };
};

const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

describe('HOWWebSocket reconnect recovery (real socket stack, fake tunnel)', () => {
  let adapter: ReturnType<typeof createAxiosHttpOverWebsocketAdapter>;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'log').mockImplementation(() => {});
    FakeTunnelSocket.instances = [];
    FakeTunnelSocket.nextModes = [];
    FakeTunnelSocket.defaultMode = {};
    configureHOW({ getProxyToken: async () => 'jwt', isTokenExpired: () => false });
    adapter = createAxiosHttpOverWebsocketAdapter({
      region: 'SG',
      WebSocket: FakeTunnelSocket as unknown as typeof WebSocket,
      customParser: (body) => body,
    });
  });

  afterEach(() => {
    adapter.dispose();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  const expectOk = async (request: Tracked<any>, within = 3_000) => {
    await advance(within);
    expect(request.state(), String(request.error()?.message)).toBe('resolved');
    await expect(request.promise).resolves.toMatchObject({ status: 200, data: '{"retCode":0}' });
  };

  it('token auth: drop with a request in flight rejects promptly, and new requests succeed after reconnect', async () => {
    await expectOk(track(adapter(instrumentsInfo())));
    const first = FakeTunnelSocket.latest();
    expect(first.sent.filter((f) => f.type === 'rpc.auth')).toHaveLength(1);

    first.mode = { holdHttp: true };
    const inFlight = track(adapter(instrumentsInfo()));
    await advance(50);
    expect(inFlight.state()).toBe('pending');
    first.serverDrop();
    await advance(1);
    expect(inFlight.state()).toBe('rejected');
    expect(String(inFlight.error()?.message)).toContain('TradeTunnel connection lost');

    // RWS backs off up to ~5 s before redialing; the server's serverTime tick then lands.
    await expectOk(track(adapter(instrumentsInfo())), 10_000);
    expect(FakeTunnelSocket.instances).toHaveLength(2);
    expect(FakeTunnelSocket.latest().sent.filter((f) => f.type === 'rpc.auth')).toHaveLength(1);
  });

  it('IP-whitelisted auth: recovers after a drop without sending a token', async () => {
    FakeTunnelSocket.defaultMode = { autoAuth: true };
    await expectOk(track(adapter(instrumentsInfo())));
    FakeTunnelSocket.latest().serverDrop();
    await expectOk(track(adapter(instrumentsInfo())), 10_000);
    expect(FakeTunnelSocket.instances).toHaveLength(2);
  });

  it('drop while a request waits for the rpc.auth reply: later requests resend the token and succeed', async () => {
    FakeTunnelSocket.nextModes = [{ ignoreAuthFrames: Infinity }];
    const waitingOnAuth = track(adapter(instrumentsInfo(30_000)));
    await advance(2_000);
    expect(FakeTunnelSocket.latest().authFramesSeen).toBe(1);
    FakeTunnelSocket.latest().serverDrop();
    await advance(8_000);
    expect(FakeTunnelSocket.instances).toHaveLength(2);
    await expectOk(track(adapter(instrumentsInfo())));
    // The waiter rides the new connection's auth reply.
    await advance(100);
    expect(waitingOnAuth.state()).toBe('resolved');
  });

  it('drop between the socket check and the token send: the token is sent on the new connection', async () => {
    FakeTunnelSocket.nextModes = [{ ignoreAuthFrames: Infinity }];
    await advance(0);
    const request = track(adapter(instrumentsInfo()));
    // Socket opens at +10 ms, first serverTime at +1010 ms; drop just before the token can go out.
    await advance(1_005);
    FakeTunnelSocket.latest().serverDrop();
    await expectOk(request, 10_000);
  });

  it('a Bad auth reply survives neither the reconnect nor the single-flight', async () => {
    let tokens = 0;
    configureHOW({ getProxyToken: async () => `jwt-${++tokens}`, isTokenExpired: () => false });
    await expectOk(track(adapter(instrumentsInfo())));
    const first = FakeTunnelSocket.latest();
    // Next token is refused, and the connection drops while the refresh waits.
    first.mode = { ignoreAuthFrames: Infinity };
    first.serverSend({ type: 'rpc.auth', error: 'ExpiredSignature' });
    await advance(10);
    const badAuth = track(adapter(instrumentsInfo()));
    await advance(700);
    first.serverDrop();
    await advance(10_000);
    await expectOk(track(adapter(instrumentsInfo())));
    await advance(100);
    expect(badAuth.state(), String(badAuth.error()?.stack)).toBe('resolved');
  });

  it('a proxy token that never becomes valid fails as missing proxy authorization, not a bare timeout', async () => {
    configureHOW({ getProxyToken: async () => 'expired-jwt', isTokenExpired: () => true });
    const request = track(adapter(instrumentsInfo()));
    await advance(21_000);
    expect(request.state()).toBe('rejected');
    expect(request.error()).toMatchObject({ name: 'ProxyTokenUnavailableError' });
    expect(String(request.error()?.message)).toContain('Tealstreet proxy authorization missing');
    expect(FakeTunnelSocket.latest().sent.filter((f) => f.type === 'rpc.auth')).toHaveLength(0);
  });

  // --- Stall: an unanswered rpc.auth pins isTokenSent for the connection's life ---
  it('an rpc.auth the server never answers is resent, so later requests do not all wait out their deadline', async () => {
    // The server only authenticates after the client resends its token.
    FakeTunnelSocket.nextModes = [{ ignoreAuthFrames: 1 }];
    const first = track(adapter(instrumentsInfo()));
    await advance(21_000);
    const retries = [0, 1, 2].map(() => track(adapter(instrumentsInfo())));
    await advance(21_000);
    // The connection itself stayed healthy throughout: serverTime ticks and pongs kept arriving.
    expect(FakeTunnelSocket.instances).toHaveLength(1);
    const socket = FakeTunnelSocket.latest();
    expect(socket.readyState).toBe(1);
    expect(
      { states: [first, ...retries].map((r) => r.state()), authFrames: socket.authFramesSeen },
      'requests after the unanswered token',
    ).toEqual({ states: ['resolved', 'resolved', 'resolved', 'resolved'], authFrames: 2 });
  });

  // --- Stall: a socket that opens and goes silent before its first pong is never replaced ---
  it('a reconnected socket that never speaks is torn down and redialed', async () => {
    await expectOk(track(adapter(instrumentsInfo())));
    FakeTunnelSocket.nextModes = [{ silent: true }];
    FakeTunnelSocket.latest().serverDrop();
    await advance(10_000);
    expect(FakeTunnelSocket.instances).toHaveLength(2);
    expect(FakeTunnelSocket.latest().readyState).toBe(1);

    // Every request on the silent socket waits out its deadline; it must be redialed within ~25 s.
    await advance(30_000);
    expect(FakeTunnelSocket.instances.length, 'sockets dialed after a silent open').toBeGreaterThan(2);
    await expectOk(track(adapter(instrumentsInfo())), 10_000);
  });

  // --- Readiness: a slow reconnect must not spend a long read's own time ---
  it('authorization ready after 25 s lets a 90 s discovery read with a 50 s body complete', async () => {
    FakeTunnelSocket.defaultMode = { ignoreAuthFrames: Infinity };
    const read = track(adapter(instrumentsInfo(90_000)));
    await advance(25_000);
    // Finish authorization independently of reconnect jitter and token retry cadence.
    const socket = FakeTunnelSocket.latest();
    expect(socket.readyState).toBe(FakeTunnelSocket.OPEN);
    socket.mode = { replyAfterMs: 50_000 };
    socket.serverSend({ type: 'rpc.auth', status: 'success' });
    await advance(15_000);
    expect(read.state(), 'body still in flight').toBe('pending');
    await advance(45_000);
    expect(read.state(), String(read.error()?.message)).toBe('resolved');
  });

  it.each(['legacy', 'managed'] as const)('%s readiness fails at 30 s without sending HTTP', async (mode) => {
    FakeTunnelSocket.defaultMode = { neverOpen: true };
    const config = instrumentsInfo(90_000);
    const read = track<unknown>(mode === 'managed' ? adapter.prepare(config) : adapter(config));
    await advance(30_500);
    expect(read.state()).toBe('rejected');
    expect(read.error()).toMatchObject({ name: 'TradeTunnelReadinessError' });
    expect(read.error()).toBeInstanceOf(TradeTunnelTimeoutError);
    expect(String(read.error()?.message)).toContain('was not ready within 30 s');
    expect(FakeTunnelSocket.instances.flatMap((socket) => socket.sent).filter((f) => f.type === 'rpc.http')).toEqual(
      [],
    );
  });

  it('a readiness failure names what it was waiting for and the socket loop state', async () => {
    FakeTunnelSocket.defaultMode = { neverOpen: true };
    const neverOpened = track(adapter(instrumentsInfo(90_000)));
    await advance(30_500);
    expect(neverOpened.error()).toMatchObject({
      name: 'TradeTunnelReadinessError',
      readiness: { stage: 'connect', lanePort: '443', tunnelOutage: false },
    });
    expect(['connecting', 'retry-scheduled']).toContain(neverOpened.error().readiness.socketPhase);
    expect(String(neverOpened.error().message)).toMatch(
      /^HTTP tunnel was not ready within 30 s: \S+ \(waiting for the socket to open; socket (connecting|retry-scheduled) for \d+ s, lane 443\)$/,
    );

    // The pending dial finally opens, to a tunnel that never answers rpc.auth.
    FakeTunnelSocket.defaultMode = { ignoreAuthFrames: Infinity };
    FakeTunnelSocket.latest().mode = FakeTunnelSocket.defaultMode;
    FakeTunnelSocket.latest().serverAccept();
    const unanswered = track(adapter(instrumentsInfo(90_000)));
    await advance(31_000);
    expect(unanswered.error()).toMatchObject({
      name: 'TradeTunnelReadinessError',
      readiness: { stage: 'authReply', socketPhase: 'connected' },
    });
    expect(String(unanswered.error().message)).toContain('waiting for the tunnel to accept the proxy token');
  });

  it('a request waiting on a socket that is backing off redials it at once, once per wait', async () => {
    FakeTunnelSocket.defaultMode = { neverOpen: true };
    const read = track(adapter(instrumentsInfo(90_000)));
    await advance(100);
    expect(FakeTunnelSocket.instances).toHaveLength(1);
    // The 4 s connect timeout fails the dial; the loop schedules a retry at least 1 s out.
    await advance(4_000);
    await advance(100);
    expect(FakeTunnelSocket.instances, 'redialed without waiting out the backoff').toHaveLength(2);
    // The redial also fails: this wait has spent its redial, so the loop backs off as before.
    await advance(4_000);
    await advance(100);
    expect(FakeTunnelSocket.instances).toHaveLength(2);
    expect(read.state()).toBe('pending');
  });

  it('a proxy token fetch still pending at the readiness cap fails as missing proxy authorization', async () => {
    configureHOW({ getProxyToken: () => new Promise<string>(() => {}), isTokenExpired: () => false });
    const read = track(adapter(instrumentsInfo(90_000)));
    await advance(30_500);
    expect(read.state()).toBe('rejected');
    expect(read.error()).toMatchObject({ name: 'ProxyTokenUnavailableError' });
    expect(String(read.error()?.message)).toContain('Tealstreet proxy authorization missing');
  });

  it.each(['legacy', 'managed'] as const)('%s keeps a caller deadline shorter than the readiness cap', async (mode) => {
    FakeTunnelSocket.defaultMode = { neverOpen: true };
    const config = instrumentsInfo(8_000);
    const read = track<unknown>(mode === 'managed' ? adapter.prepare(config) : adapter(config));
    await advance(8_000);
    expect(read.state()).toBe('rejected');
    expect(read.error()).toMatchObject({ name: 'TradeTunnelTimeoutError' });
    expect(FakeTunnelSocket.instances.flatMap((socket) => socket.sent).filter((f) => f.type === 'rpc.http')).toEqual(
      [],
    );
  });
});
