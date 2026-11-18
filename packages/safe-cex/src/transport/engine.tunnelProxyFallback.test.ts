// REST for a tunnel account while this realm's tunnel is unreachable on 8443 and 443:
// new requests take the HTTP proxy (same fleet egress), prepared ones keep the tunnel.
import type { HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { TunnelProxyFallbackNotice } from './engine';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  noteTunnelDialFailed,
  noteTunnelDialOpened,
  resetTunnelLane,
  TOKYO_HOW_EGRESS,
  tunnelDialUrl,
} from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { createEngineTransport } from './engine';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';

const tunnels = vi.hoisted(() => [] as Array<Array<Record<string, any>>>);
// false: the tunnel socket cannot open, so a preparation waits in readiness.
const tunnelSocket = vi.hoisted(() => ({ open: true }));
const tunnelReply = vi.hoisted(() => ({
  status: 200,
  data: '{"via":"tunnel"}',
  headers: {} as Record<string, string>,
}));

vi.mock('@tealstreet/http-over-websocket/src/HOWWebSocket', () => ({
  HOWWebSocket: class {
    ws = { OPEN: 1, readyState: 1, loopState: { phase: 'connected', since: 0 } };
    epoch = 0;
    get isOpen() {
      return tunnelSocket.open;
    }
    isInitialized = true;
    isTimeSynced = true;
    isTokenSent = true;
    authState = 'Good';
    listeners = new Map<string, (data: unknown) => void>();
    sent: Array<Record<string, any>> = [];
    constructor() {
      tunnels.push(this.sent);
    }
    once(event: string, listener: (data: unknown) => void) {
      this.listeners.set(event, listener);
    }
    off(event: string) {
      this.listeners.delete(event);
    }
    sendPrepared(frame: string) {
      this.send(JSON.parse(frame));
    }
    send(message: Record<string, any>) {
      this.sent.push(message);
      queueMicrotask(() => {
        this.listeners.get(`sub:${message.nonce}`)?.({
          status: tunnelReply.status,
          data_raw: tunnelReply.data,
          headers: tunnelReply.headers,
        });
        this.listeners.delete(`sub:${message.nonce}`);
      });
    }
  },
}));
vi.mock('../accessToken', () => ({
  getProxyToken: vi.fn(async () => 'proxy-fixture'),
  isTokenExpired: () => false,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => async () => 'proxy-fixture',
  ProxyTokenUnavailableError: class extends Error {},
  refreshProxyToken: vi.fn(async () => undefined),
}));

const PROXY = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/';
const proxied = { status: 200, headers: '', body: new TextEncoder().encode('{"via":"proxy"}') };

const rawTransport = (reply: HttpResponse = proxied) => {
  const dispatched: Array<{ url: string; headers: Record<string, string>; body?: unknown }> = [];
  const raw: Transport = {
    now: () => 0,
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
    request: vi.fn(async (request) => {
      dispatched.push(request);
      return reply;
    }),
    prepareHttp: vi.fn((prepare) => ({
      bind: (network: any) => ({
        ready: () => true,
        dispatch: async () => {
          dispatched.push({ ...network, url: network.url ?? prepare.url });
          return reply;
        },
        cancel: () => undefined,
      }),
      cancel: () => undefined,
    })) as never,
  };
  return { raw, dispatched };
};

const tunnelSent = () => tunnels.flat().filter((frame) => frame.type === 'rpc.http');
const body = async (response: HttpResponse) => {
  if (response.body instanceof Uint8Array) return new TextDecoder().decode(response.body);
  let text = '';
  for await (const chunk of response.body) text += new TextDecoder().decode(chunk);
  return text;
};

/** Two failed 8443 handshakes move the lane to 443; two more there make the tunnel unreachable. */
const tunnelGoesDown = () => {
  const lane = tunnelDialUrl(TOKYO_HOW_EGRESS);
  noteTunnelDialFailed(lane);
  noteTunnelDialFailed(lane);
  noteTunnelDialFailed(TOKYO_HOW_EGRESS);
  noteTunnelDialFailed(TOKYO_HOW_EGRESS);
};

const bybitTunnel = (raw: Transport, notices: TunnelProxyFallbackNotice[] = []) =>
  createEngineTransport(
    'bybit',
    { key: 'fixture-key', secret: 'fixture-secret', transport: tealstreetTransport, tealstreetTunnel: true },
    raw,
    undefined,
    undefined,
    (notice) => notices.push(notice),
  );

beforeEach(() => {
  tunnelSocket.open = true;
  tunnelReply.status = 200;
  tunnelReply.data = '{"via":"tunnel"}';
  tunnelReply.headers = {};
  resetTunnelLane(true);
  for (const sent of tunnels) sent.length = 0;
});

afterEach(() => {
  resetTunnelLane();
});

describe('REST while the tunnel is unreachable', () => {
  const replies = [
    { status: 429, text: '', limited: true },
    { status: 429, text: '<html>Too many requests</html>', limited: true },
    { status: 429, text: '{"retCode":10001}', limited: true },
    { status: 429, text: '{"retCode":10006}', limited: true },
    { status: 200, text: '{"retCode":10006}', limited: true },
    { status: 200, text: '{"retCode":10001}', limited: false },
    { status: 503, text: 'unavailable', limited: false },
  ];
  const cases = replies.flatMap((reply) =>
    [false, true].flatMap((managed) =>
      ['tunnel', 'buffered fallback', 'streamed fallback'].map((via) => ({ ...reply, managed, via })),
    ),
  );
  it.each(cases)(
    'observes $status/$text via $via (managed=$managed) exactly once',
    async ({ status, text, limited, managed, via }) => {
      tunnelReply.status = status;
      tunnelReply.data = text;
      const bytes = new TextEncoder().encode(text);
      const { raw, dispatched } = rawTransport({
        status,
        headers: '',
        body:
          via === 'streamed fallback'
            ? (async function* () {
                yield bytes.subarray(0, 10);
                yield bytes.subarray(10);
              })()
            : bytes,
      });
      const report = vi.fn();
      const route = createEngineTransport(
        'bybit',
        { key: 'fixture-key', secret: 'fixture-secret', transport: tealstreetTransport, tealstreetTunnel: true },
        raw,
        undefined,
        undefined,
        undefined,
        report,
      );
      const packet = {
        request: 1,
        method: 'GET',
        url: 'https://api.bybit.com/v5/market/kline?category=linear&symbol=BTCUSDT&interval=1',
        headers: {},
        intent: 'public' as const,
      };
      try {
        if (via !== 'tunnel') tunnelGoesDown();
        const reply = managed
          ? await (await route.prepareHttp!(packet)).bind(packet).dispatch()
          : await route.request(packet);
        expect(reply.status).toBe(status);
        expect(await body(reply)).toBe(text);
        if (limited)
          expect(report).toHaveBeenCalledExactlyOnceWith({
            retCode: status === 429 ? undefined : 10006,
            httpStatus: status === 429 ? 429 : undefined,
            egress: 'unknown',
            target: 'unknown',
          });
        else expect(report).not.toHaveBeenCalled();
        expect(tunnelSent()).toHaveLength(via === 'tunnel' ? 1 : 0);
        expect(dispatched).toHaveLength(via === 'tunnel' ? 0 : 1);
      } finally {
        route.dispose!();
      }
    },
  );

  it('reports public 10006 on the HTTP proxy fallback without reissuing the read', async () => {
    const text = '{"retCode":10006,"retMsg":"Too many visits"}';
    const { raw, dispatched } = rawTransport({
      status: 200,
      headers: 'x-tealstreet-egress: proxy-02\nx-bapi-limit-status: 0',
      body: new TextEncoder().encode(text),
    });
    const report = vi.fn();
    const route = createEngineTransport(
      'bybit',
      { key: 'fixture-key', secret: 'fixture-secret', transport: tealstreetTransport, tealstreetTunnel: true },
      raw,
      undefined,
      undefined,
      undefined,
      report,
    );
    tunnelGoesDown();
    const packet = {
      request: 1,
      method: 'GET',
      url: 'https://api.bybit.com/v5/market/kline?category=linear&symbol=BTCUSDT&interval=1',
      headers: {},
      intent: 'public' as const,
    };
    try {
      const reply = await (await route.prepareHttp!(packet)).bind(packet).dispatch();
      expect(await body(reply)).toBe(text);
      expect(report).toHaveBeenCalledExactlyOnceWith({
        retCode: 10006,
        egress: 'proxy-02',
        target: 'unknown',
        remaining: '0',
      });
      expect(dispatched.map((request) => request.url)).toEqual([`${PROXY}${packet.url}`]);
      expect(tunnelSent()).toHaveLength(0);
    } finally {
      route.dispose!();
    }
  });

  it('captures 10006 headers after genuine managed HOW bind and dispatch, excluding private intent', async () => {
    tunnelReply.data = '{"retCode":10006,"retMsg":"Too many visits"}';
    tunnelReply.headers = { 'x-tealstreet-egress': 'node-a', 'x-bapi-limit-status': '0' };
    const report = vi.fn();
    const { raw } = rawTransport();
    const route = createEngineTransport(
      'bybitinverse',
      { key: 'fixture-key', secret: 'fixture-secret', transport: tealstreetTransport, tealstreetTunnel: true },
      raw,
      undefined,
      undefined,
      undefined,
      report,
    );
    const packet = {
      request: 1,
      method: 'GET',
      url: 'https://api.bybit.com/v5/market/kline?category=inverse&symbol=BTCUSD&interval=1',
      headers: {},
      intent: 'public' as const,
    };
    try {
      const bound = (await route.prepareHttp!(packet)).bind(packet);
      expect(report).not.toHaveBeenCalled();
      expect(bound.ready()).toBe(true);
      const response = await bound.dispatch();
      expect(await body(response)).toBe(tunnelReply.data);
      expect(report).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ egress: 'node-a', remaining: '0' }));
      const privatePacket = {
        ...packet,
        request: 2,
        intent: 'primary' as const,
        headers: { 'X-BAPI-API-KEY': 'private' },
      };
      await (await route.prepareHttp!(privatePacket)).bind(privatePacket).dispatch();
      expect(report).toHaveBeenCalledTimes(1);
      expect(tunnelSent()).toHaveLength(2);
    } finally {
      route.dispose!();
    }
  });

  it('stays on the tunnel while the tunnel is reachable', async () => {
    const { raw, dispatched } = rawTransport();
    const notices: TunnelProxyFallbackNotice[] = [];
    const route = bybitTunnel(raw, notices);
    const response = await route.request({ method: 'GET', url: 'https://api.bybit.com/v5/market/time', headers: {} });
    expect(await body(response)).toBe('{"via":"tunnel"}');
    expect(dispatched).toHaveLength(0);
    expect(notices).toHaveLength(0);
    route.dispose!();
  });

  it('sends new requests through the HTTP proxy, reported once per outage, and returns to the tunnel when it opens', async () => {
    const { raw, dispatched } = rawTransport();
    const notices: TunnelProxyFallbackNotice[] = [];
    const route = bybitTunnel(raw, notices);
    tunnelGoesDown();
    const url = 'https://api.bybit.com/v5/position/list?category=linear';
    await route.request({ method: 'GET', url, headers: { 'X-BAPI-SIGN': 'signed' } });
    await route.request({ method: 'GET', url, headers: { 'X-BAPI-SIGN': 'signed' } });
    expect(tunnelSent()).toHaveLength(0);
    expect(dispatched.map((request) => request.url)).toEqual([`${PROXY}${url}`, `${PROXY}${url}`]);
    expect(dispatched[0].headers['X-BAPI-SIGN']).toBe('signed');
    expect(Object.values(dispatched[0].headers)).toContain('Bearer proxy-fixture');
    expect(notices).toEqual([
      { outage: expect.objectContaining({ portsTried: '8443+443', failures: 4, episode: expect.any(Number) }) },
    ]);

    noteTunnelDialOpened(TOKYO_HOW_EGRESS);
    const response = await route.request({ method: 'GET', url, headers: {} });
    expect(await body(response)).toBe('{"via":"tunnel"}');
    expect(dispatched).toHaveLength(2);

    tunnelGoesDown();
    await route.request({ method: 'GET', url, headers: {} });
    expect(notices).toHaveLength(2);
    expect(notices[1].outage.episode).toBeGreaterThan(notices[0].outage.episode);
    route.dispose!();
  });

  it('keeps a write prepared before the outage on the tunnel and never resends it through the proxy', async () => {
    const { raw, dispatched } = rawTransport();
    const route = bybitTunnel(raw);
    const original = {
      request: 7,
      method: 'POST',
      url: 'https://api.bybit.com/v5/order/create',
      headers: {},
      intent: 'primary' as const,
    };
    const prepared = await route.prepareHttp!(original);
    tunnelGoesDown();
    const bound = prepared.bind({ ...original, headers: { 'X-BAPI-SIGN': 'exact' }, body: '{"qty":1}' });
    const response = await bound.dispatch();
    expect(await body(response)).toBe('{"via":"tunnel"}');
    expect(tunnelSent()).toHaveLength(1);
    expect(tunnelSent()[0].arg).toMatchObject({ url: original.url, data: '{"qty":1}' });
    expect(dispatched).toHaveLength(0);
    expect(raw.prepareHttp).not.toHaveBeenCalled();
    route.dispose!();
  });

  it('prepares a write after the outage through the HTTP proxy', async () => {
    const { raw, dispatched } = rawTransport();
    const route = bybitTunnel(raw);
    tunnelGoesDown();
    const original = {
      request: 8,
      method: 'POST',
      url: 'https://api.bybit.com/v5/order/create',
      headers: {},
      intent: 'primary' as const,
    };
    const prepared = await route.prepareHttp!(original);
    await prepared.bind({ ...original, headers: { 'X-BAPI-SIGN': 'exact' }, body: '{"qty":1}' }).dispatch();
    expect(tunnelSent()).toHaveLength(0);
    expect(dispatched).toHaveLength(1);
    expect(dispatched[0].url).toBe(`${PROXY}${original.url}`);
    route.dispose!();
  });

  it('leaves a venue host the proxy fleet refuses on the tunnel, tagged', async () => {
    const { raw, dispatched } = rawTransport();
    const notices: TunnelProxyFallbackNotice[] = [];
    const route = bybitTunnel(raw, notices);
    tunnelGoesDown();
    await route.request({ method: 'GET', url: 'https://api.example.com/v5/market/time', headers: {} });
    expect(tunnelSent()).toHaveLength(1);
    expect(dispatched).toHaveLength(0);
    expect(notices).toEqual([{ outage: expect.any(Object), skipped: 'proxy_not_allowlisted' }]);
    route.dispose!();
  });

  it('never activates outside a browser realm', async () => {
    resetTunnelLane(false);
    const { raw, dispatched } = rawTransport();
    const route = bybitTunnel(raw);
    tunnelGoesDown();
    await route.request({ method: 'GET', url: 'https://api.bybit.com/v5/market/time', headers: {} });
    expect(dispatched).toHaveLength(0);
    route.dispose!();
  });
});

describe('a public read waiting on a tunnel that is failing', () => {
  const lane = () => tunnelDialUrl(TOKYO_HOW_EGRESS);
  const instruments = (request: number, intent: 'public' | 'primary' = 'public') => ({
    request,
    method: 'GET',
    url: 'https://api.bybit.com/v5/market/instruments-info?category=linear',
    headers: {},
    intent,
    timeoutMs: 60_000,
  });
  const settled = <T>(promise: Promise<T>) => {
    const state = { done: false };
    void promise.then(
      () => (state.done = true),
      () => (state.done = true),
    );
    return state;
  };

  it('moves to the HTTP proxy at the first failed 443 handshake, having sent nothing on the tunnel', async () => {
    vi.useFakeTimers();
    try {
      tunnelSocket.open = false;
      const { raw, dispatched } = rawTransport();
      const route = bybitTunnel(raw);
      const original = instruments(11);
      const preparing = Promise.resolve(route.prepareHttp!(original));
      const state = settled(preparing);
      noteTunnelDialFailed(lane());
      noteTunnelDialFailed(lane());
      await vi.advanceTimersByTimeAsync(5_000);
      expect(state.done, 'the lane falling back to 443 is not a failing tunnel yet').toBe(false);
      noteTunnelDialFailed(TOKYO_HOW_EGRESS);
      await vi.advanceTimersByTimeAsync(100);
      const prepared = await preparing;
      await prepared.bind(original).dispatch();
      expect(tunnelSent()).toHaveLength(0);
      expect(dispatched.map((request) => request.url)).toEqual([`${PROXY}${original.url}`]);
      // The reroute keeps the read's own deadline rather than starting a new one.
      expect(vi.mocked(raw.prepareHttp!).mock.calls[0][0].timeoutMs).toBeLessThanOrEqual(55_000);
      route.dispose!();
    } finally {
      vi.useRealTimers();
    }
  });

  it('takes the HTTP proxy at once when the tunnel is already failing', async () => {
    const { raw, dispatched } = rawTransport();
    const route = bybitTunnel(raw);
    noteTunnelDialFailed(lane());
    noteTunnelDialFailed(lane());
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    const original = instruments(12);
    await (await route.prepareHttp!(original)).bind(original).dispatch();
    expect(dispatched.map((request) => request.url)).toEqual([`${PROXY}${original.url}`]);
    route.dispose!();
  });

  it('leaves a signed read waiting on the tunnel', async () => {
    vi.useFakeTimers();
    try {
      tunnelSocket.open = false;
      const { raw } = rawTransport();
      const route = bybitTunnel(raw);
      const state = settled(Promise.resolve(route.prepareHttp!(instruments(13, 'primary'))));
      noteTunnelDialFailed(lane());
      noteTunnelDialFailed(lane());
      noteTunnelDialFailed(TOKYO_HOW_EGRESS);
      await vi.advanceTimersByTimeAsync(1_000);
      expect(state.done).toBe(false);
      expect(raw.prepareHttp).not.toHaveBeenCalled();
      route.dispose!();
    } finally {
      vi.useRealTimers();
    }
  });
});
