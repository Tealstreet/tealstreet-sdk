import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { directTransport } from './direct';
import { createEngineTransport, resolveEngineRoutingPlan } from './engine';
import { setDefaultTransport } from './registry';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { getPublicCorsProxy } from './tealstreet/transportResolution';

const mocks = vi.hoisted(() => ({ token: vi.fn(async () => 'proxy-jwt') }));
vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: mocks.token,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => () => mocks.token(),
}));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => false }));

const makeRaw = () => ({
  webSocketHeaders: true,
  now: vi.fn(() => 42),
  request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
  connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
  setTimeout: vi.fn<Transport['setTimeout']>(),
  clearTimeout: vi.fn<Transport['clearTimeout']>(),
});

const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'venue-key',
  secret: 'venue-secret',
  transport: tealstreetTransport,
  ...extra,
});

// Legacy's lighter.types.ts: every profile but robinhood-testnet sends REST through the public CORS proxy.
const PROXIED: Array<[ExchangeName, boolean, string]> = [
  ['lighter', false, 'https://mainnet.zklighter.elliot.ai'],
  ['lighter', true, 'https://testnet-api.lighter.xyz'],
  ['lighter-robinhood', false, 'https://api.rh.lighter.xyz'],
];
const UNSET_OR_OFF: Array<Partial<ExchangeOptions>> = [{}, { tealstreetProxy: false }, { tealstreetProxy: true }];

afterEach(() => {
  setDefaultTransport(directTransport);
  mocks.token.mockClear();
});

describe('Lighter REST goes through the public CORS proxy, as legacy did', () => {
  it.each(PROXIED)(
    '%s (testnet %s) proxies every write and read on the page transport',
    async (exchange, testnet, base) => {
      for (const extra of UNSET_OR_OFF) {
        const raw = makeRaw();
        const routed = createEngineTransport(exchange, options({ testnet, ...extra }), raw);
        await routed.request({
          method: 'POST',
          url: `${base}/api/v1/sendTx`,
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: 'tx_type=14&tx_info=%7B%7D',
        });
        await routed.request({ method: 'GET', url: `${base}/api/v1/account?by=index&value=1`, headers: {} });
        const [write, read] = raw.request.mock.calls.map(([sent]) => sent);
        expect(write.url).toBe(`${getPublicCorsProxy()}/${base}/api/v1/sendTx`);
        expect(read.url).toBe(`${getPublicCorsProxy()}/${base}/api/v1/account?by=index&value=1`);
        for (const sent of [write, read]) {
          expect(Object.keys(sent.headers).map((name) => name.toLowerCase())).not.toContain('authorization');
          expect(Object.keys(sent.headers)).not.toContain('Tealstreet-Authorization');
        }
      }
      expect(mocks.token).not.toHaveBeenCalled();
    },
  );

  it.each(PROXIED)('%s (testnet %s) gives the native host the same unauthorized prefix', (exchange, testnet, base) => {
    for (const extra of UNSET_OR_OFF) {
      const plan = resolveEngineRoutingPlan(exchange, options({ testnet, ...extra }), base);
      expect(plan.http.prefix).toBe(getPublicCorsProxy());
      expect(plan.http.authorize).toBe(false);
    }
  });

  it('leaves robinhood-testnet direct, as legacy did', async () => {
    const base = 'https://api.rh-testnet.lighter.xyz';
    const raw = makeRaw();
    await createEngineTransport('lighter-robinhood', options({ testnet: true }), raw).request({
      method: 'POST',
      url: `${base}/api/v1/sendTx`,
      headers: {},
      body: 'tx_type=14',
    });
    expect(raw.request.mock.calls[0][0].url).toBe(`${base}/api/v1/sendTx`);
    expect(resolveEngineRoutingPlan('lighter-robinhood', options({ testnet: true }), base).http.prefix).toBe('');
  });

  it('leaves a direct-transport account direct: it has no public CORS proxy', async () => {
    const base = 'https://mainnet.zklighter.elliot.ai';
    const raw = makeRaw();
    await createEngineTransport('lighter', options({ transport: directTransport }), raw).request({
      method: 'GET',
      url: `${base}/api/v1/account`,
      headers: {},
    });
    expect(raw.request.mock.calls[0][0].url).toBe(`${base}/api/v1/account`);
  });
});
