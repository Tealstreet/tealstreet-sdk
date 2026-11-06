// A keyless Bybit chart source on a proxied or tunnelled account reads through the public CORS
// proxy: Bybit keeps its tunnel under publicDataOnly, which egressed from the trading fleet.
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { directTransport } from './direct';
import { createEngineTransport } from './engine';
import { setDefaultTransport } from './registry';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { getPrivateProxyHost, getPublicCorsProxy } from './tealstreet/transportResolution';

const tunnels = vi.hoisted(() => [] as Array<Array<Record<string, any>>>);
// false: the tunnel socket cannot open, so a preparation waits in readiness.
const tunnelSocket = vi.hoisted(() => ({ open: true }));
const tunnelReply = vi.hoisted(() => ({ data: '{"via":"tunnel"}', headers: {} as Record<string, string> }));

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
          status: 200,
          data_raw: tunnelReply.data,
          headers: tunnelReply.headers,
        });
        this.listeners.delete(`sub:${message.nonce}`);
      });
    }
  },
}));
const mocks = vi.hoisted(() => ({ token: vi.fn(async () => 'proxy-jwt') }));
vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: mocks.token,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => () => mocks.token(),
}));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => false }));

const KLINE = 'https://api.bybit.com/v5/market/kline?category=linear&symbol=BTCUSDT&interval=5&limit=500';

const makeRaw = () => ({
  webSocketHeaders: true,
  now: vi.fn(() => 42),
  request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
  connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
  setTimeout: vi.fn<Transport['setTimeout']>(),
  clearTimeout: vi.fn<Transport['clearTimeout']>(),
});

/** A worker mirror's chart source: keyless, public data only, the account's routing flags. */
const chartSource = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: '',
  secret: '',
  transport: tealstreetTransport,
  publicDataOnly: true,
  tealstreetProxy: true,
  tealstreetTunnel: true,
  ...extra,
});

let framesBefore = 0;
/** Tunnel frames sent since this test began; the tunnel socket may outlive one engine. */
const tunnelFrames = () => tunnels.flat().length - framesBefore;

const sent = async (exchange: ExchangeName, options: ExchangeOptions, url = KLINE, headers = {}) => {
  const raw = makeRaw();
  const routed = createEngineTransport(exchange, options, raw);
  try {
    await routed.request({ method: 'GET', url, headers });
  } finally {
    routed.dispose?.();
  }
  return raw.request.mock.calls[0]?.[0];
};

beforeEach(() => {
  framesBefore = tunnels.flat().length;
});
afterEach(() => {
  setDefaultTransport(directTransport);
  mocks.token.mockClear();
});

describe('Bybit keyless chart reads leave the trading fleet through the public CORS proxy', () => {
  it.each([
    ['proxy and tunnel', { tealstreetProxy: true, tealstreetTunnel: true }],
    ['tunnel only', { tealstreetProxy: false, tealstreetTunnel: true }],
    ['proxy only', { tealstreetProxy: true, tealstreetTunnel: false }],
  ])('sends klines on a %s account through the public proxy, never the tunnel', async (_name, flags) => {
    const request = await sent('bybit', chartSource(flags));
    expect(request?.url).toBe(`${getPublicCorsProxy()}/${KLINE}`);
    expect(Object.keys(request?.headers ?? {}).some((name) => name.toLowerCase() === 'authorization')).toBe(false);
    expect(tunnelFrames()).toBe(0);
    expect(mocks.token).not.toHaveBeenCalled();
  });

  it.each(['bybitspot', 'bybitinverse'] as const)('routes %s the same way', async (exchange) => {
    const url = KLINE.replace('category=linear', exchange === 'bybitspot' ? 'category=spot' : 'category=inverse');
    expect((await sent(exchange, chartSource()))?.url).toBe(`${getPublicCorsProxy()}/${KLINE}`);
    expect((await sent(exchange, chartSource(), url))?.url).toBe(`${getPublicCorsProxy()}/${url}`);
  });

  it('leaves a direct account browser-direct', async () => {
    const request = await sent('bybit', chartSource({ tealstreetProxy: false, tealstreetTunnel: false }));
    expect(request?.url).toBe(KLINE);
  });

  it('keeps a keyed engine on its tunnel', async () => {
    const request = await sent('bybit', chartSource({ key: 'venue-key', secret: 'venue-secret' }));
    expect(request).toBeUndefined();
    expect(tunnelFrames()).toBeGreaterThan(0);
  });

  it('keeps a host the public proxy does not list for the venue on the exchange proxy', async () => {
    const demo = KLINE.replace('api.bybit.com', 'api-demo.bybit.com');
    const request = await sent('bybitdemo', chartSource({ tealstreetTunnel: false }), demo);
    expect(request?.url).toBe(`${getPrivateProxyHost('asia')}/${demo}`);
  });

  it('routes a managed public read on a tunnel account the same way', async () => {
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array() }));
    const bind = vi.fn((_packet: Parameters<Transport['request']>[0]) => ({ ready: () => true, dispatch }));
    const routed = createEngineTransport('bybit', chartSource(), { ...makeRaw(), prepareHttp: async () => ({ bind }) });
    const packet = { request: 1, method: 'GET', url: KLINE, headers: {} };
    try {
      const prepared = await routed.prepareHttp!({ ...packet, intent: 'public' });
      await prepared.bind(packet).dispatch();
      expect(bind.mock.calls[0]?.[0]).toMatchObject({ url: `${getPublicCorsProxy()}/${KLINE}` });
      expect(tunnelFrames()).toBe(0);
    } finally {
      routed.dispose?.();
    }
  });

  it('still refuses a credentialed request toward the public proxy before any I/O', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', chartSource(), raw);
    try {
      await expect(
        routed.request({ method: 'GET', url: KLINE, headers: { 'X-BAPI-API-KEY': 'venue-key' } }),
      ).rejects.toThrow('refused: signed request to the public proxy');
      expect(raw.request).not.toHaveBeenCalled();
      expect(tunnelFrames()).toBe(0);
    } finally {
      routed.dispose?.();
    }
  });
});
