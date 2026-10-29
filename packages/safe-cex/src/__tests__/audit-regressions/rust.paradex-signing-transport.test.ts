import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { paradexSigningTransport } from '../../exchanges/rust/paradex.signing';

const runtime = vi.hoisted(() => ({ node: true }));
vi.mock('../../utils/runtime', () => ({ isNodeOrBunRuntime: () => runtime.node }));
afterEach(() => { runtime.node = true; vi.unstubAllGlobals(); });

const fixture = (sign: RemoteSigner['sign'], httpMode?: Transport['httpMode']) => {
  const request = vi.fn(async (_request: Parameters<Transport['request']>[0]) => ({ status: 200, body: new Uint8Array() }));
  const transport: Transport = {
    httpMode,
    now: () => 0, connect: () => ({ send() {}, close() {} }),
    setTimeout, clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>), request,
  };
  const remoteSigner: RemoteSigner = { sign, accountName: 'account', isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return { request, transport: paradexSigningTransport({ remoteSigner } as ExchangeOptions, transport) };
};
const input = {
  method: 'POST', url: 'https://api.prod.paradex.trade/v1/orders',
  headers: { Authorization: 'Bearer jwt', 'PARADEX-PARTNER': 'TEALSTREET' }, body: '{"size":"2"}',
};

describe('Paradex engine remote HTTP signing', () => {
  it.each([
    ['direct', false, 'http://signed.example/orders'],
    ['direct', true, undefined],
    ['how', false, 'https://api.prod.paradex.trade/v1//signed.example/orders'],
    ['how', true, 'https://api.prod.paradex.trade/v1//signed.example/orders'],
  ] as const)('preserves protocol-relative TS URLs for %s with node=%s', async (mode, node, expected) => {
    runtime.node = node;
    vi.stubGlobal('location', new URL('http://app.example/account'));
    if (node) {
      vi.stubGlobal('window', undefined);
      vi.stubGlobal('document', undefined);
    }
    const { request, transport } = fixture(async ({ id }) => ({
      id, config: { url: '//signed.example/orders', headers: {} },
    }), mode);
    if (expected === undefined) {
      await expect(transport.request(input)).rejects.toThrow('Invalid URL');
      expect(request).not.toHaveBeenCalled();
    } else {
      await transport.request(input);
      expect(request.mock.calls[0][0].url).toBe(expected);
    }
  });

  it.each([true, false])('resolves client URLs in a renderer or worker with node=%s', async (node) => {
    runtime.node = node;
    vi.stubGlobal('window', node ? { location: new URL('https://desktop.example/account') } : undefined);
    vi.stubGlobal('document', node ? {} : undefined);
    vi.stubGlobal('location', new URL('https://worker.example/account'));
    const { request, transport } = fixture(async ({ id }) => ({
      id, config: { url: '//signed.example/orders', headers: {} },
    }), 'direct');
    await transport.request(input);
    expect(request.mock.calls[0][0].url).toBe('https://signed.example/orders');
  });

  it('passes the TS adapter contract and applies returned bytes before account routing', async () => {
    const sign = vi.fn<RemoteSigner['sign']>(async (request) => ({
      id: request.id, config: { url: '/orders', params: { market: 'BTC-USD-PERP' }, data: { signature: 'opaque', size: '2' }, headers: { 'x-signer': 'yes' } },
    }));
    const { request, transport } = fixture(sign);
    await transport.request(input);
    expect(sign.mock.calls[0][0]).toMatchObject({
      type: 'http', exchangeName: 'paradex', accountName: 'account',
      config: { baseURL: 'https://api.prod.paradex.trade/v1', url: '/orders', method: 'POST', data: { size: '2' }, headers: input.headers },
    });
    expect(request).toHaveBeenCalledWith(expect.objectContaining({ ...input,
      url: 'https://api.prod.paradex.trade/v1/orders?market=BTC-USD-PERP',
      body: '{"signature":"opaque","size":"2"}', headers: { 'x-signer': 'yes', authorization: 'Bearer jwt' },
    }));
  });

  it.each(['/auth', '/auth/0x456'])('keeps JWT fetch %s outside remote HTTP signing', async (path) => {
    const sign = vi.fn<RemoteSigner['sign']>();
    const { request, transport } = fixture(sign);
    const auth = { ...input, url: `https://api.prod.paradex.trade/v1${path}` };
    await transport.request(auth);
    expect(sign).not.toHaveBeenCalled();
    expect(request).toHaveBeenCalledWith(auth);
  });

  it('honors removed params/data and usable replacement bearer headers', async () => {
    const { request, transport } = fixture(async ({ id, config }) => {
      expect(config?.params).toEqual({ cursor: 'a+b', page_size: '100' });
      return { id, config: { headers: { AUTHORIZATION: 'Bearer fresh' } } };
    });
    await transport.request({ ...input, method: 'GET', url: `${input.url}?cursor=a%2Bb&page_size=100` });
    expect(request.mock.calls[0][0]).toMatchObject({ url: input.url, body: undefined, headers: { authorization: 'Bearer fresh' } });
  });

  it('serializes returned arrays with the TS Paradex comma convention', async () => {
    const { request, transport } = fixture(async ({ id }) => ({ id, config: {
      headers: {}, params: { market: ['BTC-USD-PERP', 'ETH-USD-PERP'] },
    } }));
    await transport.request(input);
    expect(request.mock.calls[0][0].url).toBe(`${input.url}?market=BTC-USD-PERP%2CETH-USD-PERP`);
  });

  it.each(['throw', 'error', 'mismatch', 'missing'] as const)('never sends on signer %s', async (kind) => {
    const { request, transport } = fixture(async ({ id }) => {
      if (kind === 'throw') throw new Error('unavailable');
      if (kind === 'error') return { id, error: 'unavailable' };
      if (kind === 'mismatch') return { id: 'wrong', config: { headers: {} } };
      return { id };
    });
    await expect(transport.request(input)).rejects.toThrow();
    expect(request).not.toHaveBeenCalled();
  });

  it('does not send after disposal or deadline while the signer is pending', async () => {
    const controller = new AbortController();
    const { request, transport } = fixture(async ({ id }) => {
      controller.abort(new Error('disposed'));
      return { id, config: { headers: {} } };
    });
    await expect(transport.request({ ...input, signal: controller.signal })).rejects.toThrow('disposed');
    expect(request).not.toHaveBeenCalled();
  });

  it('bounds a signer that never replies and ignores its late result', async () => {
    let finish!: (response: Awaited<ReturnType<RemoteSigner['sign']>>) => void;
    let id = '';
    const { request, transport } = fixture((input) => {
      id = input.id;
      return new Promise((resolve) => { finish = resolve; });
    });
    await expect(transport.request({ ...input, timeoutMs: 5 })).rejects.toThrow();
    finish({ id, config: { headers: {} } });
    await Promise.resolve();
    expect(request).not.toHaveBeenCalled();
  });
});
