import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { directTransport } from './direct';
import { createEngineTransport } from './engine';
import { setDefaultTransport } from './registry';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { getPrivateProxyHost, getPublicCorsProxy } from './tealstreet/transportResolution';

const mocks = vi.hoisted(() => ({ token: vi.fn(async () => 'proxy-jwt') }));
vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: mocks.token,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => () => mocks.token(),
}));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => false }));

const KLINES = 'https://open-api.bingx.com/openApi/swap/v3/quote/klines?symbol=BTC-USDT&interval=5m&limit=500';

const makeRaw = () => ({
  webSocketHeaders: true,
  now: vi.fn(() => 42),
  request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
  connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
  setTimeout: vi.fn<Transport['setTimeout']>(),
  clearTimeout: vi.fn<Transport['clearTimeout']>(),
});

/** A worker mirror's chart source: keyless, public data only, the account's forced proxy flag. */
const chartSource = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: '',
  secret: '',
  transport: tealstreetTransport,
  publicDataOnly: true,
  tealstreetProxy: true,
  tealstreetTunnel: true,
  ...extra,
});

const sent = async (exchange: ExchangeName, options: ExchangeOptions, method = 'GET', url = KLINES) => {
  const raw = makeRaw();
  await createEngineTransport(exchange, options, raw).request({ method, url, headers: {} });
  expect(raw.request).toHaveBeenCalledOnce();
  return raw.request.mock.calls[0][0];
};

afterEach(() => {
  setDefaultTransport(directTransport);
  mocks.token.mockClear();
});

describe('a keyless public-data engine reads through the public CORS proxy, off the trading fleet', () => {
  it('sends BingX chart history through the public proxy without the proxy token', async () => {
    const request = await sent('bingx', chartSource());
    expect(request.url).toBe(`${getPublicCorsProxy()}/${KLINES}`);
    expect(Object.keys(request.headers).some((name) => name.toLowerCase() === 'authorization')).toBe(false);
    expect(mocks.token).not.toHaveBeenCalled();
  });

  it('keeps a keyed engine and a full account on the exchange proxy', async () => {
    const exchangeProxy = getPrivateProxyHost('asia');
    for (const options of [
      chartSource({ key: 'venue-key', secret: 'venue-secret' }),
      chartSource({ publicDataOnly: false }),
    ]) {
      const request = await sent('bingx', { ...options, tealstreetTunnel: false });
      expect(request.url.startsWith(`${exchangeProxy}/`), JSON.stringify(options.key)).toBe(true);
    }
  });

  it('never moves a venue with its own direct public reads onto the public proxy', async () => {
    const request = await sent('hyperliquid', chartSource(), 'GET', 'https://api.hyperliquid.xyz/info');
    expect(request.url.startsWith(`${getPublicCorsProxy()}/`)).toBe(false);
  });

  // /api/public_proxy is a host allowlist: Crypto.com and BloFin demo are not on it (403).
  it('keeps a venue or host the public proxy refuses on the exchange proxy', async () => {
    const exchangeProxy = getPrivateProxyHost('asia');
    for (const [exchange, url] of [
      ['cryptocom', 'https://api.crypto.com/exchange/v1/public/get-candlestick?instrument_name=BTCUSD-PERP'],
      ['blofin', 'https://demo-trading-openapi.blofin.com/api/v1/market/candles?instId=BTC-USDT'],
    ] as const) {
      const request = await sent(exchange, chartSource({ tealstreetTunnel: false }), 'GET', url);
      expect(request.url, exchange).toBe(`${exchangeProxy}/${url}`);
    }
    const mainnet = 'https://openapi.blofin.com/api/v1/market/candles?instId=BTC-USDT';
    expect((await sent('blofin', chartSource(), 'GET', mainnet)).url).toBe(`${getPublicCorsProxy()}/${mainnet}`);
  });

  it('routes a managed public read (the chart path) the same way', async () => {
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array() }));
    const bind = vi.fn((_packet: Parameters<Transport['request']>[0]) => ({ ready: () => true, dispatch }));
    const routed = createEngineTransport('bingx', chartSource(), { ...makeRaw(), prepareHttp: async () => ({ bind }) });
    const packet = { request: 1, method: 'GET', url: KLINES, headers: {} };
    try {
      const prepared = await routed.prepareHttp!({ ...packet, intent: 'public' });
      await prepared.bind(packet).dispatch();
      expect(bind.mock.calls[0]?.[0]).toMatchObject({ url: `${getPublicCorsProxy()}/${KLINES}` });
      expect(dispatch).toHaveBeenCalledOnce();
      expect(mocks.token).not.toHaveBeenCalled();
    } finally {
      routed.dispose?.();
    }
  });

  it('still refuses a credentialed request toward the public proxy before any I/O', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bingx', chartSource(), raw);
    await expect(
      routed.request({ method: 'GET', url: `${KLINES}&timestamp=1&signature=abc`, headers: {} }),
    ).rejects.toThrow('refused: signed request to the public proxy');
    expect(raw.request).not.toHaveBeenCalled();
  });
});
