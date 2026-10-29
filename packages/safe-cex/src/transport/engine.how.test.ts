import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { setTunnelEgressUrls } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { createEngineTransport } from './engine';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { AWS_PUBLIC_CORS_PROXY } from './tealstreet/transportResolution';

const tunnels = vi.hoisted(
  () =>
    [] as Array<{
      url: string | (() => string);
      state: {
        sent: Array<Record<string, any>>;
        reply: { status: number; data_raw: string; headers: Record<string, string> };
      };
    }>,
);

// Since AWS_EXIT.md step 3e both regions are one tunnel host, so a test that tells
// the regions apart pins distinct ones (the HOW singletons resolve their url lazily).
const TOKYO_HOW_EGRESS = 'wss://jp.tunnel.invalid/tunnel';
const SINGAPORE_HOW_EGRESS = 'wss://sg.tunnel.invalid/tunnel';
setTunnelEgressUrls({ jp: TOKYO_HOW_EGRESS, sg: SINGAPORE_HOW_EGRESS });
const tunnelState = (host: string) =>
  tunnels.find(({ url }) => (typeof url === 'function' ? url() : url) === host)!.state;

// Exercise the real HOW adapter and its RPC envelope; replace only the wire.
vi.mock('@tealstreet/http-over-websocket/src/HOWWebSocket', () => ({
  HOWWebSocket: class {
    epoch = 0;
    isOpen = true;
    isInitialized = true;
    isTimeSynced = true;
    isTokenSent = true;
    authState = 'Good';
    listeners = new Map<string, (data: unknown) => void>();
    state = { sent: [] as Array<Record<string, any>>, reply: { status: 200, data_raw: '', headers: {} } };
    constructor(url: string | (() => string)) {
      tunnels.push({ url, state: this.state });
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
      this.state.sent.push(message);
      queueMicrotask(() => {
        this.listeners.get(`sub:${message.nonce}`)?.(this.state.reply);
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
  bindProxyTokenLookup: () => async () => '',
}));

const rawTransport = (): Transport => ({
  now: () => 0,
  setTimeout,
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
  request: vi.fn(async () => {
    throw new Error('unexpected direct HTTP');
  }),
});

describe('engine through the real HOW package', () => {
  beforeEach(() => {
    for (const { state } of tunnels) {
      state.sent.length = 0;
      state.reply = { status: 200, data_raw: '', headers: {} };
    }
  });

  it.each([
    ['bingx', {}, SINGAPORE_HOW_EGRESS],
    ['bybit', { tealstreetTunnel: true, proxyRegion: 'JA' }, TOKYO_HOW_EGRESS],
    ['bybit', { tealstreetTunnel: true, proxyRegion: 'SG' }, SINGAPORE_HOW_EGRESS],
    ['okx', { tealstreetTunnel: true }, SINGAPORE_HOW_EGRESS],
    ['breakoutprop', { tealstreetTunnel: true }, TOKYO_HOW_EGRESS],
    ['tealstreet-dex', { tealstreetTunnel: true }, SINGAPORE_HOW_EGRESS],
    ['kodiakfi', { tealstreetTunnel: true }, SINGAPORE_HOW_EGRESS],
    ['mode', { tealstreetTunnel: true, testnet: true }, SINGAPORE_HOW_EGRESS],
    ['salsadex', { tealstreetTunnel: true }, SINGAPORE_HOW_EGRESS],
    ['what', { tealstreetTunnel: true }, SINGAPORE_HOW_EGRESS],
    ['woofi', { tealstreetTunnel: true }, SINGAPORE_HOW_EGRESS],
  ] as const)('sends %s to the resolved tunnel with signed bytes intact', async (exchange, options, host) => {
    const raw = rawTransport();
    const route = createEngineTransport(
      exchange,
      { key: 'fixture-key', secret: 'fixture-secret', ...options, transport: tealstreetTransport },
      raw,
    );
    const state = tunnelState(host);
    const responseBody = ' { "orderId": 900719925474099312345, "price": 1.2300 }\n';
    state.reply = { status: 429, data_raw: responseBody, headers: { 'retry-after': '2' } };
    const orderly = ['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'].includes(exchange);
    const origin = orderly
      ? `https://${'testnet' in options && options.testnet ? 'testnet-' : ''}api-evm.orderly.org`
      : 'https://api.example.com';
    const url = `${origin}/order?a=1&a=2&encoded=%2F+%20&signature=abc`;
    const body = ' {"id":900719925474099312345, "price":1.2300}\n';
    const headers = { 'Content-Type': 'application/json',
      [exchange === 'breakoutprop' ? 'Authorization' : orderly ? 'orderly-signature' : 'X-Venue-Signature']:
        exchange === 'breakoutprop' ? 'DXAPI documented-session' : 'signed' };
    const response = await route.request({
      method: 'POST',
      url,
      body,
      headers,
    });
    expect(raw.request).not.toHaveBeenCalled();
    expect(state.sent).toHaveLength(1);
    expect(state.sent[0]).toEqual({
      type: 'rpc.http',
      rpc_version: 4,
      format: 'raw',
      accept_encoding: ['gzip'],
      accept_binary: true,
      nonce: expect.any(String),
      arg: {
        url,
        method: 'POST',
        params: {},
        data: body,
        headers,
      },
    });
    expect(response.status).toBe(429);
    expect(response.headers).toBe('retry-after: 2');
    expect(new TextDecoder().decode(response.body as Uint8Array)).toBe(responseBody);
  });

  it('keeps overrideProxyUrl and Brazil headers inside the selected HOW RPC', async () => {
    const route = createEngineTransport(
      'bybit',
      {
        key: 'fixture-key',
        secret: 'fixture-secret',
        transport: tealstreetTransport,
        tealstreetProxy: true,
        tealstreetTunnel: true,
        overrideProxyUrl: 'https://override.example',
        bybitSiteId: 'BRA_BTL',
      },
      rawTransport(),
    );
    await route.request({ method: 'GET', url: 'https://api.bybit.com/v5/market/kline?category=linear', headers: {} });
    expect(tunnelState(SINGAPORE_HOW_EGRESS).sent[0].arg).toEqual({
      url: 'https://override.example/https://api.bybit.com/v5/market/kline?category=linear',
      method: 'GET',
      params: {},
      data: {},
      headers: { 'x-site-id': 'BRA_BTL' },
    });
  });
  it('Aster secondary reads keep AWS CORS inside the real HOW RPC while writes keep the primary path', async () => {
    const raw = rawTransport();
    const route = createEngineTransport(
      'asterdex',
      {
        key: 'primary-wallet',
        secret: 'primary-key',
        secondary_key: 'read-wallet',
        secondary_secret: 'read-key',
        tealstreetTunnel: true,
        transport: tealstreetTransport,
      },
      raw,
    );
    const state = tunnelState(TOKYO_HOW_EGRESS);
    state.reply = { status: 200, data_raw: '{"balance":"1.2300"}', headers: {} };
    const url = 'https://fapi.asterdex.com/fapi/v3/account?user=owner&nonce=1001&signature=exact%2Bbytes';
    await route.request({ method: 'GET', url, headers: { 'X-MBX-APIKEY': 'read-wallet' } });
    expect(state.sent.at(-1)?.arg).toMatchObject({
      method: 'GET',
      url: `${AWS_PUBLIC_CORS_PROXY}/${url}`,
      headers: { 'X-MBX-APIKEY': 'read-wallet' },
    });
    await route.request({ method: 'POST', url, headers: { 'X-MBX-APIKEY': 'primary-wallet' } });
    expect(state.sent.at(-1)?.arg).toMatchObject({
      method: 'POST',
      url,
      headers: { 'X-MBX-APIKEY': 'primary-wallet' },
    });
    expect(raw.request).not.toHaveBeenCalled();
    route.dispose!();
  });
});

describe('managed engine through real HOW preparation', () => {
  it('prepares without rpc.http and synchronously dispatches the final signed bytes exactly once', async () => {
    const raw = rawTransport();
    const routed = createEngineTransport(
      'bybit',
      { key: 'fixture-key', secret: 'fixture-secret', transport: tealstreetTransport, tealstreetTunnel: true },
      raw,
    );
    const state = tunnelState(SINGAPORE_HOW_EGRESS);
    state.sent.length = 0;
    state.reply = { status: 200, data_raw: ' {"id":900719925474099312345}\n', headers: {} };
    const original = {
      request: 31,
      method: 'POST',
      url: 'https://api.bybit.com/v5/order/create',
      headers: {},
      intent: 'primary' as const,
    };
    const prepared = await routed.prepareHttp!(original);
    expect(state.sent).toHaveLength(0);
    const signed = {
      ...original,
      url: original.url + '?x=%2B+%20',
      headers: { 'X-BAPI-SIGN': 'exact' },
      body: ' {"id":900719925474099312345,"price":1.2300}\n',
    };
    const bound = prepared.bind(signed);
    expect(bound.ready()).toBe(true);
    expect(state.sent).toHaveLength(0);
    const pending = bound.dispatch();
    expect(state.sent).toHaveLength(1);
    expect(state.sent[0].arg).toEqual({
      url: signed.url,
      method: 'POST',
      params: {},
      data: signed.body,
      headers: signed.headers,
    });
    expect(new TextDecoder().decode((await pending).body as Uint8Array)).toBe(state.reply.data_raw);
    expect(raw.request).not.toHaveBeenCalled();
    expect(() => bound.dispatch()).toThrow();
    routed.dispose!();
  });
});
